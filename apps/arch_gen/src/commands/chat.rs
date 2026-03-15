use design_search_engine::RankedCandidate;

use crate::input_bridge::arch_state_to_architecture;
use crate::output::narrative::verbalize_candidate;
use crate::input_bridge::{SavedEvaluation, SavedCandidate, SavedCodeMetrics};
use code_ir::{ArchitectureToCodeIR, DeterministicArchitectureToCodeIR};

// ─── 質問分類 ─────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum QuestionKind {
    Compare,      // 比較: 違い / vs / どちら
    Reason,       // 理由: なぜ / おすすめ / 適切
    Fitness,      // 適合性: 向いて / 合う / 適して
    Structure,    // 構造: 構成 / 要素 / 含まれ
    Weakness,     // 弱点: 問題 / 欠点 / 改善
    Unknown,
}

fn classify(question: &str) -> QuestionKind {
    let q = question.to_lowercase();

    if q.contains("違い") || q.contains("差") || q.contains("比べ")
        || q.contains("比較") || q.contains(" vs ") || q.contains("どちら")
        || q.contains("compared") || q.contains("difference")
    {
        QuestionKind::Compare
    } else if q.contains("なぜ") || q.contains("理由") || q.contains("おすすめ")
        || q.contains("選ぶ") || q.contains("適切") || q.contains("why")
        || q.contains("recommend") || q.contains("best")
    {
        QuestionKind::Reason
    } else if q.contains("向いて") || q.contains("合う") || q.contains("適して")
        || q.contains("合わせ") || q.contains("suit") || q.contains("fit")
    {
        QuestionKind::Fitness
    } else if q.contains("構成") || q.contains("要素") || q.contains("含まれ")
        || q.contains("どんな") || q.contains("structure") || q.contains("component")
    {
        QuestionKind::Structure
    } else if q.contains("問題") || q.contains("欠点") || q.contains("弱点")
        || q.contains("改善") || q.contains("weak") || q.contains("problem")
        || q.contains("issue")
    {
        QuestionKind::Weakness
    } else {
        QuestionKind::Unknown
    }
}

// ─── ChatEngine ────────────────────────────────────────────────────────────────

pub struct ChatEngine<'a> {
    pub requirement: &'a str,
    pub candidates: &'a [RankedCandidate],
    pub selected: Option<usize>,
}

impl<'a> ChatEngine<'a> {
    pub fn respond(&self, question: &str) -> String {
        match classify(question) {
            QuestionKind::Compare   => self.respond_compare(),
            QuestionKind::Reason    => self.respond_reason(),
            QuestionKind::Fitness   => self.respond_fitness(question),
            QuestionKind::Structure => self.respond_structure(),
            QuestionKind::Weakness  => self.respond_weakness(),
            QuestionKind::Unknown   => self.respond_unknown(question),
        }
    }

    // ─── 比較 ─────────────────────────────────────────────────────────────

    fn respond_compare(&self) -> String {
        if self.candidates.len() < 2 {
            return "比較するには候補が2つ以上必要です。".to_string();
        }

        let mut out = String::from("【候補の比較】\n");
        let all_names: Vec<Vec<String>> = self.candidates.iter()
            .map(|c| candidate_names(c))
            .collect();

        for (i, names) in all_names.iter().enumerate() {
            out.push_str(&format!("\n案 {}: {} 要素\n", i + 1, names.len()));
            out.push_str(&format!("  構成: {}\n", names.join(", ")));
        }

        // 差分をピックアップ
        out.push_str("\n【構成の差異】\n");
        let base = &all_names[0];
        for (i, names) in all_names.iter().enumerate().skip(1) {
            let only_in_base: Vec<_> = base.iter()
                .filter(|n| !names.contains(n))
                .collect();
            let only_in_other: Vec<_> = names.iter()
                .filter(|n| !base.contains(n))
                .collect();
            if !only_in_base.is_empty() || !only_in_other.is_empty() {
                out.push_str(&format!("  案1 vs 案{}: ", i + 1));
                if !only_in_base.is_empty() {
                    let names_str: Vec<_> = only_in_base.iter().map(|s| s.as_str()).collect();
                    out.push_str(&format!("案1のみ=[{}] ", names_str.join(", ")));
                }
                if !only_in_other.is_empty() {
                    let names_str: Vec<_> = only_in_other.iter().map(|s| s.as_str()).collect();
                    out.push_str(&format!("案{}のみ=[{}]", i + 1, names_str.join(", ")));
                }
                out.push('\n');
            } else {
                out.push_str(&format!("  案1 と 案{}: 要素構成は同じ（順序のみ異なる可能性）\n", i + 1));
            }
        }

        out
    }

    // ─── 理由・おすすめ ───────────────────────────────────────────────────

    fn respond_reason(&self) -> String {
        let Some(top) = self.candidates.first() else {
            return "候補がありません。先に設計を生成してください。".to_string();
        };
        let eval = &top.state.world_state.evaluation;
        let best_axis = best_axis_label(eval.structural_quality, eval.dependency_quality,
                                        eval.constraint_satisfaction, eval.simulation_quality);
        let names = candidate_names(top);

        let mut out = String::from("【案1 を推奨する理由】\n");
        out.push_str(&format!("最も高いスコア軸: {best_axis}\n\n"));
        out.push_str(&format!("構成要素: {}\n", names.join(", ")));
        out.push_str("\n設計観点からのコメント:\n");

        let saved = to_saved_candidate(1, top);
        let narrative = verbalize_candidate(self.requirement, &saved);
        for line in narrative.lines() {
            out.push_str(&format!("  {line}\n"));
        }

        if self.candidates.len() > 1 {
            out.push_str("\n他の候補と比べた場合:\n");
            for (i, c) in self.candidates.iter().enumerate().skip(1) {
                let score_diff = top.score - c.score;
                out.push_str(&format!(
                    "  案{} よりスコアが {:.3} 高い\n", i + 1, score_diff
                ));
            }
        }
        out
    }

    // ─── 適合性 ───────────────────────────────────────────────────────────

    fn respond_fitness(&self, question: &str) -> String {
        let q = question.to_lowercase();

        // 質問に含まれる要求ワードを候補コンポーネントと照合
        let requirement_words = extract_keywords(&q);

        let mut out = String::from("【要求との適合性】\n");
        for (i, c) in self.candidates.iter().enumerate() {
            let names = candidate_names(c);
            let matched: Vec<_> = requirement_words.iter()
                .filter(|w| names.iter().any(|n| n.to_lowercase().contains(w.as_str())))
                .collect();
            let match_rate = if requirement_words.is_empty() {
                0.0
            } else {
                matched.len() as f64 / requirement_words.len() as f64
            };

            out.push_str(&format!("\n案{}: ", i + 1));
            if match_rate > 0.5 {
                out.push_str("要求と高い適合性\n");
            } else if match_rate > 0.0 {
                out.push_str("部分的に適合\n");
            } else {
                out.push_str("追加の絞り込みが必要\n");
            }
            out.push_str(&format!("  要素: {}\n", names.join(", ")));
        }

        out.push_str("\nより詳しく絞り込むには追加要件を入力してください。\n");
        out
    }

    // ─── 構造説明 ─────────────────────────────────────────────────────────

    fn respond_structure(&self) -> String {
        let idx = self.selected.unwrap_or(0);
        let Some(c) = self.candidates.get(idx) else {
            return "候補を選択してください（`s <N>`）。".to_string();
        };

        let names = candidate_names(c);
        let architecture = arch_state_to_architecture(&c.state.architecture_state);
        let units_by_id = architecture.design_units_by_id();
        let deps: Vec<(String, String)> = architecture.dependencies.iter()
            .filter_map(|dep| {
                let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
                let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
                Some((from, to))
            })
            .collect();

        let mut out = format!("【案{} の構成】\n", idx + 1);
        out.push_str(&format!("{} 要素で構成されています。\n\n", names.len()));

        out.push_str("コンポーネント一覧:\n");
        for (i, name) in names.iter().enumerate() {
            let role = component_role_label(name);
            out.push_str(&format!("  {}. {} — {}\n", i + 1, name, role));
        }

        if !deps.is_empty() {
            out.push_str("\n依存関係:\n");
            for (from, to) in &deps {
                out.push_str(&format!("  {} → {}\n", from, to));
            }
        } else {
            out.push_str("\n依存関係: まだ明示されていません。\n");
            out.push_str("追加要件を入力するか `r` でrefineすると依存関係が現れます。\n");
        }
        out
    }

    // ─── 弱点・改善 ──────────────────────────────────────────────────────

    fn respond_weakness(&self) -> String {
        let idx = self.selected.unwrap_or(0);
        let Some(c) = self.candidates.get(idx) else {
            return "候補を選択してください（`s <N>`）。".to_string();
        };

        let eval = &c.state.world_state.evaluation;
        let mut issues = Vec::new();

        if eval.dependency_quality < 0.5 {
            issues.push("依存関係の品質が低い — コンポーネント間の結合が強すぎる可能性があります。");
        }
        if eval.structural_quality < 0.7 {
            issues.push("構造品質に改善の余地あり — 役割分担の再整理を検討してください。");
        }
        if eval.constraint_satisfaction < 0.8 {
            issues.push("制約充足度が低い — 要件のうち満たせていない条件がある可能性があります。");
        }
        if eval.complexity > 0.6 {
            issues.push("複雑度が高い — コンポーネントを増やしすぎている可能性があります。");
        }
        if eval.simulation_quality < 0.7 {
            issues.push("シミュレーション品質が低い — 動的な振る舞いに懸念があります。");
        }

        let mut out = format!("【案{} の弱点・改善点】\n", idx + 1);
        if issues.is_empty() {
            out.push_str("現時点では明確な弱点は検出されていません。\n");
            out.push_str("さらに深掘りするには追加要件を入力してください。\n");
        } else {
            for issue in &issues {
                out.push_str(&format!("• {issue}\n"));
            }
            out.push_str("\n改善するには `r` で追加要件を入力して再探索してください。\n");
        }
        out
    }

    // ─── 不明 ──────────────────────────────────────────────────────────────

    fn respond_unknown(&self, question: &str) -> String {
        let idx = self.selected.unwrap_or(0);
        if let Some(c) = self.candidates.get(idx) {
            let saved = to_saved_candidate(idx + 1, c);
            let narrative = verbalize_candidate(self.requirement, &saved);
            let mut out = format!(
                "質問「{}」に対する直接的な回答は難しいですが、現在の案について説明します。\n\n",
                question
            );
            for line in narrative.lines() {
                out.push_str(&format!("  {line}\n"));
            }
            out.push_str("\n以下のような質問を試してください:\n");
            out.push_str("  「案の違いは？」「なぜ案1がいい？」「構成を説明して」「改善点は？」\n");
            out
        } else {
            "まず設計を生成してから質問してください。".to_string()
        }
    }
}

// ─── ヘルパー ──────────────────────────────────────────────────────────────────

fn candidate_names(c: &RankedCandidate) -> Vec<String> {
    let architecture = arch_state_to_architecture(&c.state.architecture_state);
    let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);
    code_ir.modules.iter().map(|m| m.name.clone()).collect()
}

fn to_saved_candidate(id: usize, c: &RankedCandidate) -> SavedCandidate {
    let eval = &c.state.world_state.evaluation;
    let names = candidate_names(c);
    SavedCandidate {
        id,
        score: c.score,
        pareto_rank: c.pareto_rank,
        evaluation: SavedEvaluation {
            structural_quality: eval.structural_quality,
            dependency_quality: eval.dependency_quality,
            constraint_satisfaction: eval.constraint_satisfaction,
            complexity: eval.complexity,
            simulation_quality: eval.simulation_quality,
            total: eval.total(),
        },
        components: names,
        dependencies: vec![],
        code_metrics: SavedCodeMetrics::default(),
    }
}

fn best_axis_label(structural: f64, dependency: f64, constraint: f64, simulation: f64) -> &'static str {
    let max = structural.max(dependency).max(constraint).max(simulation);
    if (max - structural).abs() < 1e-9 { "構造品質 (structural_quality)" }
    else if (max - dependency).abs() < 1e-9 { "依存品質 (dependency_quality)" }
    else if (max - constraint).abs() < 1e-9 { "制約充足 (constraint_satisfaction)" }
    else { "シミュレーション品質 (simulation_quality)" }
}

fn component_role_label(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("controller") { "入力処理・画面制御を担う層" }
    else if n.contains("service") { "ビジネスロジック・編集機能をまとめる層" }
    else if n.contains("repository") { "永続化・データアクセスを仲介する層" }
    else if n.contains("database") || n.contains("db") { "データを保存するストレージ層" }
    else if n.contains("gateway") || n.contains("api") { "外部との境界・ルーティング層" }
    else if n.contains("event") || n.contains("bus") { "イベント配信・非同期通信層" }
    else { "汎用コンポーネント" }
}

fn extract_keywords(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

// ─── チャットセッションの REPL ループ ─────────────────────────────────────────

/// インタラクティブモードから呼び出すチャットサブループ。
pub fn run_chat_session(
    requirement: &str,
    candidates: &[RankedCandidate],
    selected: Option<usize>,
    stdin: &std::io::Stdin,
    out: &mut impl std::io::Write,
) {
    use std::io::BufRead;

    writeln!(out, "【チャットモード】設計について質問してください（'q' で戻る）").ok();
    writeln!(out, "  例: 「案の違いは？」「なぜ案1がいい？」「構成を説明して」「改善点は？」").ok();
    writeln!(out).ok();

    let engine = ChatEngine { requirement, candidates, selected };

    loop {
        write!(out, "arch_gen [chat]> ").ok();
        out.flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            _ => {}
        }
        let input = line.trim();
        if input.is_empty() { continue; }
        if input == "q" || input == "quit" || input == "exit" {
            writeln!(out, "チャットモードを終了しました。").ok();
            break;
        }

        let response = engine.respond(input);
        writeln!(out, "\n{response}").ok();
    }
}

// ─── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_compare() {
        assert_eq!(classify("案1と案2の違いは？"), QuestionKind::Compare);
        assert_eq!(classify("案1 vs 案2"), QuestionKind::Compare);
    }

    #[test]
    fn test_classify_reason() {
        assert_eq!(classify("なぜ案1がおすすめ？"), QuestionKind::Reason);
        assert_eq!(classify("どれが適切ですか"), QuestionKind::Reason);
    }

    #[test]
    fn test_classify_structure() {
        assert_eq!(classify("この案の構成を説明して"), QuestionKind::Structure);
        assert_eq!(classify("どんな要素が含まれてる？"), QuestionKind::Structure);
    }

    #[test]
    fn test_classify_weakness() {
        assert_eq!(classify("改善点は？"), QuestionKind::Weakness);
        assert_eq!(classify("この設計の欠点は"), QuestionKind::Weakness);
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(classify("こんにちは"), QuestionKind::Unknown);
    }

    #[test]
    fn test_respond_no_candidates() {
        let engine = ChatEngine {
            requirement: "test",
            candidates: &[],
            selected: None,
        };
        let res = engine.respond("案の違いは？");
        assert!(res.contains("候補が2つ以上必要") || res.contains("候補がありません"));
    }

    #[test]
    fn test_component_role_labels() {
        assert!(component_role_label("controller_1").contains("入力処理"));
        assert!(component_role_label("service_2").contains("ビジネスロジック"));
        assert!(component_role_label("repository_3").contains("永続化"));
        assert!(component_role_label("database_4").contains("ストレージ"));
    }
}
