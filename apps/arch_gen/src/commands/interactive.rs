use std::io::{self, BufRead, Write};
use std::path::Path;

use design_search_engine::RankedCandidate;

use crate::commands::chat::run_chat_session;
use crate::commands::generate::{PipelineResult, run_phase9_pipeline};
use design_reasoning::{ReasoningAxis, StructuredReasoningEngine, StructuredReasoningInput};
use crate::template::{enrich_dynamic, infer_template, prompt_and_fill_dynamic};
use crate::store::{DesignStore, format_store_list};
use crate::commands::knowledge_layer::{
    StoredKnowledgeHit, grounding_knowledge_path, knowledge_layer_metrics, prepare_inference_input,
    promote_hits_to_grounding, save_temporary_web_hits, temporary_knowledge_path,
};
use crate::input_bridge::{
    GenerateRequest, SavedCandidate, SavedDesign, SavedEvaluation, arch_state_to_architecture,
    load_design_file, save_design_file,
};
use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::text::CandidateDisplay;
use code_ir::{ArchitectureToCodeIR, DeterministicArchitectureToCodeIR};

// ─── ANSI カラー ─────────────────────────────────────────────────────────────
const RST: &str  = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str  = "\x1b[2m";
const RED: &str  = "\x1b[31m";
const GRN: &str  = "\x1b[32m";
const YLW: &str  = "\x1b[33m";
const CYN: &str  = "\x1b[36m";

/// インタラクティブセッションの状態
struct SessionState {
    requirement: String,
    candidates: Vec<RankedCandidate>,
    selected: Option<usize>,
    latest_web_hits: Vec<StoredKnowledgeHit>,
}

pub struct InteractiveArgs {
    pub from: Option<String>,
}

/// `interactive` コマンド: 対話型設計精緻化フロー
pub fn run(args: InteractiveArgs) -> Result<(), String> {
    let mut state: Option<SessionState> = None;
    let mut startup_messages: Vec<String> = Vec::new();

    // --from で既存設計をロード
    if let Some(ref path) = args.from {
        match load_design_file(Path::new(path)) {
            Ok(design) => {
                startup_messages.push(format!(
                    "Loaded: \"{}\" ({} candidates)",
                    design.input,
                    design.candidates.len()
                ));
                // 既存設計から requirement だけ復元してセッション開始
                // パイプラインを再実行してランク付き候補を取得
                let req = GenerateRequest::new(
                    design.input.clone(),
                    10,
                    5,
                    design.candidates.len().max(1),
                    true,
                    false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult { ranked, .. }) => {
                        let n = ranked.len();
                        state = Some(SessionState {
                            requirement: design.input,
                            candidates: ranked,
                            selected: None,
                            latest_web_hits: Vec::new(),
                        });
                        startup_messages.push(format!("Resumed session: {n} candidates available"));
                    }
                    Err(e) => {
                        startup_messages
                            .push(format!("Warning: could not restore candidates: {e}"));
                    }
                };
            }
            Err(e) => {
                startup_messages.push(format!("Warning: {e}; starting fresh session"));
            }
        };
    }

    run_session(state, &startup_messages)
}

pub fn run_seeded(requirement: String, candidates: Vec<RankedCandidate>) -> Result<(), String> {
    run_session(
        Some(SessionState {
            requirement,
            candidates,
            selected: None,
            latest_web_hits: Vec::new(),
        }),
        &[],
    )
}

fn run_session(mut state: Option<SessionState>, startup_messages: &[String]) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let cwd = current_dir_display();
    writeln!(out, "{BOLD}{CYN}Architecture Generative AI{RST}").ok();
    writeln!(out, "  {DIM}Directory :{RST} {cwd}").ok();
    writeln!(out, "  {DIM}Mode      : interactive{RST}").ok();
    writeln!(out, "{DIM}{}{RST}", "─".repeat(60)).ok();
    writeln!(out, "  {DIM}/help でコマンド一覧  •  q で終了  •  /generate <要件> で開始{RST}").ok();
    writeln!(out, "{DIM}{}{RST}", "─".repeat(60)).ok();
    for message in startup_messages {
        writeln!(out, "{message}").ok();
    }
    if let Some(ref sess) = state {
        writeln!(out, "Requirement: \"{}\"", sess.requirement).ok();
        writeln!(
            out,
            "{} candidate(s) ready for review.",
            sess.candidates.len()
        )
        .ok();
        writeln!(
            out,
            "Type 'list' to inspect candidates. You can also type a new requirement directly."
        )
        .ok();
        print_knowledge_status(&mut out);
        writeln!(out).ok();
    }

    // REPLループ
    loop {
        // プロンプト表示（現在ディレクトリ付き）
        let cwd = current_dir_display();
        let prompt = if state.is_none() {
            format!("arch_gen ({cwd})> ")
        } else if state
            .as_ref()
            .map(|s| s.selected.is_none())
            .unwrap_or(false)
        {
            format!("arch_gen ({cwd}) [s]elect> ")
        } else {
            format!("arch_gen ({cwd}) [r/e/m/q]> ")
        };
        write!(out, "{prompt}").ok();
        out.flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Err(_) => break,
            _ => {}
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "q" | "quit" | "exit" => {
                writeln!(out, "Goodbye.").ok();
                break;
            }
            "help" | "h" | "?" => {
                print_help(
                    &mut out,
                    state.is_some(),
                    state.as_ref().and_then(|s| s.selected),
                );
            }
            // ─── スラッシュコマンド (/generate, /scan, /evaluate など) ────────
            _ if input.starts_with('/') => {
                if handle_slash_interactive(input, &mut state, &stdin, &mut out) {
                    writeln!(out, "Goodbye.").ok();
                    break;
                }
            }
            _ if state.is_none() => {
                // 要件入力モード
                let requirement = normalize_requirement_input(input);
                if requirement != input {
                    writeln!(
                        out,
                        "Interpreting input as requirement: \"{}\"",
                        requirement
                    )
                    .ok();
                }

                // ─ 推論ドリブンテンプレート補強ステップ ─
                let template = infer_template(&requirement);
                let enriched = {
                    let mut stdin_lock = stdin.lock();
                    let filled =
                        match prompt_and_fill_dynamic(&template, &mut stdin_lock, &mut out) {
                            Ok(f) => f,
                            Err(e) => {
                                writeln!(out, "\nError: {e}").ok();
                                continue;
                            }
                        };
                    enrich_dynamic(&requirement, &filled, &template)
                };

                write!(out, "Generating... ").ok();
                out.flush().ok();

                let knowledge = match prepare_inference_input(&enriched.enriched_text) {
                    Ok(knowledge) => knowledge,
                    Err(e) => {
                        writeln!(out, "\nError: {e}").ok();
                        continue;
                    }
                };
                let req = GenerateRequest::new(
                    knowledge.enriched_requirement.clone(),
                    10 + enriched.beam_width_bonus,
                    5 + enriched.max_depth_bonus,
                    3,
                    true,
                    false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult {
                        ranked,
                        search_states_count,
                    }) => {
                        writeln!(out, "{GRN}done{RST}  {DIM}({search_states_count} states){RST}").ok();
                        print_candidates(&enriched.enriched_text, &ranked, &mut out);
                        print_knowledge_context(&mut out, &knowledge);
                        state = Some(SessionState {
                            requirement: enriched.enriched_text,
                            candidates: ranked,
                            selected: None,
                            latest_web_hits: Vec::new(),
                        });
                    }
                    Err(e) => {
                        writeln!(out, "\n{RED}error:{RST} {e}").ok();
                    }
                }
            }
            _ => {
                let Some(ref mut sess) = state else {
                    writeln!(out, "Enter a requirement to start, or 'q' to quit.").ok();
                    continue;
                };

                let parts: Vec<&str> = input.splitn(2, ' ').collect();
                match parts[0] {
                    "s" | "select" => {
                        let idx: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                        if idx == 0 || idx > sess.candidates.len() {
                            writeln!(out, "Invalid index. Choose 1–{}", sess.candidates.len()).ok();
                        } else {
                            sess.selected = Some(idx - 1);
                            writeln!(out, "案 {idx} を選択しました。").ok();
                            if let Some(candidate) = sess.candidates.get(idx - 1) {
                                print_candidate_detail(&sess.requirement, idx, candidate, &mut out);
                            }
                        }
                    }
                    "r" | "refine" => {
                        write!(out, "Additional requirement: ").ok();
                        out.flush().ok();
                        let mut extra = String::new();
                        stdin.lock().read_line(&mut extra).ok();
                        let extra = extra.trim().to_string();
                        refine_session(sess, &extra, &mut out);
                    }
                    "m" | "mermaid" => {
                        let idx = sess.selected.unwrap_or(0);
                        if let Some(c) = sess.candidates.get(idx) {
                            let (names, pairs) = candidate_to_display_parts(c);
                            writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                        } else {
                            writeln!(out, "No candidate selected. Use 's <N>' first.").ok();
                        }
                    }
                    "e" | "export" => {
                        let fmt = parts.get(1).copied().unwrap_or("text");
                        let idx = sess.selected.unwrap_or(0);
                        if let Some(c) = sess.candidates.get(idx) {
                            let (names, pairs) = candidate_to_display_parts(c);
                            match fmt {
                                "mermaid" => {
                                    writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                                }
                                "markdown" => {
                                    let ev = c.state.world_state.evaluation.clone();
                                    let display = CandidateDisplay {
                                        score: c.score,
                                        pareto_rank: c.pareto_rank,
                                        component_names: names,
                                        dependency_pairs: pairs,
                                        evaluation: ev,
                                        generated_files: vec![],
                                    };
                                    writeln!(
                                        out,
                                        "{}",
                                        build_markdown(&sess.requirement, 0, &[display])
                                    )
                                    .ok();
                                }
                                _ => {
                                    print_candidate_detail(&sess.requirement, idx + 1, c, &mut out);
                                }
                            }
                        } else {
                            writeln!(out, "No candidate selected.").ok();
                        }
                    }
                    "save" => {
                        let arg = parts.get(1).copied().unwrap_or("design_session.json");
                        let saved = build_saved_design(sess);

                        // 引数がファイルパスっぽければ従来の動作、それ以外はストアに保存
                        if arg.contains('/') || arg.ends_with(".json") {
                            if let Err(e) = save_design_file(&saved, Path::new(arg)) {
                                writeln!(out, "Error: {e}").ok();
                            } else {
                                writeln!(out, "Saved to {arg}").ok();
                            }
                        } else {
                            let store = DesignStore::new();
                            match store.save(arg, &saved) {
                                Ok(path) => writeln!(out, "Saved as '{}' → {}", arg, path.display()).ok(),
                                Err(e)   => writeln!(out, "Error: {e}").ok(),
                            };
                        }
                    }
                    "saves" | "store" => {
                        let store = DesignStore::new();
                        match store.list() {
                            Ok(entries) => writeln!(out, "{}", format_store_list(&entries)).ok(),
                            Err(e)      => writeln!(out, "Error: {e}").ok(),
                        };
                    }
                    "load" => {
                        let name = match parts.get(1) {
                            Some(n) => *n,
                            None => {
                                writeln!(out, "Usage: load <name>").ok();
                                continue;
                            }
                        };
                        let store = DesignStore::new();
                        match store.load(name) {
                            Ok(design) => {
                                // 読み込んだ設計でパイプラインを再実行
                                let req = GenerateRequest::new(
                                    design.input.clone(), 10, 5,
                                    design.candidates.len().max(1), true, false,
                                );
                                match run_phase9_pipeline(&req) {
                                    Ok(PipelineResult { ranked, .. }) => {
                                        writeln!(out, "Loaded '{}' — {} candidates.", name, ranked.len()).ok();
                                        print_candidates(&design.input, &ranked, &mut out);
                                        sess.requirement = design.input;
                                        sess.candidates = ranked;
                                        sess.selected = None;
                                    }
                                    Err(e) => { writeln!(out, "Error restoring pipeline: {e}").ok(); }
                                }
                            }
                            Err(e) => { writeln!(out, "Error: {e}").ok(); }
                        }
                    }
                    "chat" => {
                        run_chat_session(
                            &sess.requirement,
                            &sess.candidates,
                            sess.selected,
                            &stdin,
                            &mut out,
                        );
                    }
                    "w" | "web" => {
                        let query = parts.get(1).copied().unwrap_or(&sess.requirement);
                        match crate::commands::web_search::search(query, 5) {
                            Ok(hits) if hits.is_empty() => {
                                writeln!(out, "Web search returned no results.").ok();
                            }
                            Ok(hits) => {
                                match save_temporary_web_hits(query, &hits) {
                                    Ok(saved_hits) => {
                                        sess.latest_web_hits = saved_hits.clone();
                                    }
                                    Err(e) => {
                                        writeln!(out, "Failed to save temporary knowledge: {e}")
                                            .ok();
                                        sess.latest_web_hits.clear();
                                    }
                                }
                                writeln!(out, "Web knowledge for: {query}").ok();
                                for (i, hit) in sess.latest_web_hits.iter().enumerate() {
                                    writeln!(out, "  {}. {}", i + 1, hit.title).ok();
                                    writeln!(out, "     {}", hit.snippet).ok();
                                }
                                if !sess.latest_web_hits.is_empty() {
                                    writeln!(
                                        out,
                                        "Temporary knowledge saved to {}",
                                        temporary_knowledge_path().display()
                                    )
                                    .ok();
                                    writeln!(
                                        out,
                                        "`g <N>` で確認済みの結果を grounding knowledge に昇格できます。"
                                    )
                                    .ok();
                                }
                            }
                            Err(e) => {
                                writeln!(out, "Web search error: {e}").ok();
                            }
                        }
                    }
                    "g" | "ground" => {
                        let idx: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                        if idx == 0 || idx > sess.latest_web_hits.len() {
                            writeln!(
                                out,
                                "Invalid index. Choose 1–{}",
                                sess.latest_web_hits.len()
                            )
                            .ok();
                            continue;
                        }
                        let hit_id = sess.latest_web_hits[idx - 1].id;
                        match promote_hits_to_grounding(&[hit_id]) {
                            Ok(promoted) if promoted.is_empty() => {
                                writeln!(out, "Selected knowledge was not promoted.").ok();
                            }
                            Ok(promoted) => {
                                writeln!(
                                    out,
                                    "Promoted {} item(s) to grounding knowledge.",
                                    promoted.len()
                                )
                                .ok();
                                writeln!(
                                    out,
                                    "Grounding knowledge saved to {}",
                                    grounding_knowledge_path()
                                        .map(|path| path.display().to_string())
                                        .unwrap_or_else(
                                            |_| ".arch_gen/grounding_knowledge.json".to_string()
                                        )
                                )
                                .ok();
                                sess.latest_web_hits.retain(|hit| hit.id != hit_id);
                            }
                            Err(e) => {
                                writeln!(out, "Grounding promotion error: {e}").ok();
                            }
                        }
                    }
                    "k" | "knowledge" => {
                        print_knowledge_status(&mut out);
                    }
                    "list" | "ls" => {
                        print_candidates(&sess.requirement, &sess.candidates, &mut out);
                    }
                    _ => {
                        let normalized = normalize_requirement_input(input);
                        if normalized != input {
                            writeln!(
                                out,
                                "Interpreting input as additional requirement: \"{}\"",
                                normalized
                            )
                            .ok();
                        }
                        refine_session(sess, &normalized, &mut out);
                    }
                }
            }
        }
    }

    Ok(())
}

// ─── ヘルパー ────────────────────────────────────────────────────────────────

fn print_help(out: &mut impl Write, has_session: bool, selected: Option<usize>) {
    writeln!(out, "Architecture Generative AI — コマンド一覧").ok();
    writeln!(out, "{}", "─".repeat(60)).ok();
    writeln!(out, "  生成・探索").ok();
    writeln!(out, "    /generate <要件>      新しい要件からアーキテクチャを生成").ok();
    writeln!(out, "    /r [追加要件]          追加要件を反映して再探索").ok();
    writeln!(out, "    /scan <ディレクトリ>   ソースコードを逆解析").ok();
    writeln!(out, "").ok();
    writeln!(out, "  候補操作").ok();
    writeln!(out, "    /list  (/ls)           候補一覧を表示").ok();
    writeln!(out, "    /s <N>  (/select <N>)  候補 N を選択").ok();
    writeln!(out, "    /m  (/mermaid)         Mermaid ダイアグラムを表示").ok();
    writeln!(out, "    /e [fmt]  (/export)    エクスポート (text|mermaid|markdown)").ok();
    writeln!(out, "    /chat                  候補について Q&A").ok();
    writeln!(out, "").ok();
    writeln!(out, "  保存・読み込み").ok();
    writeln!(out, "    /save [名前|パス]      設計を保存").ok();
    writeln!(out, "    /load <名前>           ストアから読み込み").ok();
    writeln!(out, "    /saves  (/store)       保存済み設計一覧").ok();
    writeln!(out, "").ok();
    writeln!(out, "  知識・Web").ok();
    writeln!(out, "    /w [クエリ]  (/web-search)  Web 検索").ok();
    writeln!(out, "    /g <N>  (/ground)     Web 結果を grounding に昇格").ok();
    writeln!(out, "    /k  (/knowledge)      知識レイヤーの状態表示").ok();
    writeln!(out, "    /knowledge-audit      知識レイヤーの詳細監査").ok();
    writeln!(out, "").ok();
    writeln!(out, "  ファイル操作").ok();
    writeln!(out, "    /evaluate <file>      設計ファイルをスコア評価").ok();
    writeln!(out, "    /explain <file>       設計ファイルの説明レポート").ok();
    writeln!(out, "").ok();
    writeln!(out, "  その他").ok();
    writeln!(out, "    q  /quit              終了").ok();
    writeln!(out, "    <テキスト>            要件として入力（スラッシュ不要）").ok();
    writeln!(out, "{}", "─".repeat(60)).ok();
    if !has_session {
        writeln!(out, "  セッション未開始 — /generate <要件> または要件テキストを入力").ok();
    }
    if let Some(idx) = selected {
        writeln!(out, "  現在の選択: 候補 {}", idx + 1).ok();
    }
}

fn normalize_requirement_input(input: &str) -> String {
    let trimmed = input.trim();
    let stripped = trimmed.strip_prefix("arch_gen ").unwrap_or(trimmed).trim();

    for prefix in ["/generate", "generate"] {
        if let Some(rest) = stripped.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() {
                return trimmed.to_string();
            }
            return trim_matching_quotes(rest);
        }
    }

    trim_matching_quotes(stripped)
}

fn trim_matching_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next().unwrap_or_default();
        let last = trimmed.chars().last().unwrap_or_default();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn print_candidates(requirement: &str, candidates: &[RankedCandidate], out: &mut impl Write) {
    let _ = requirement;
    writeln!(out).ok();
    for (i, c) in candidates.iter().enumerate() {
        let (names, pairs) = candidate_to_display_parts(c);
        let n = names.len();
        let title = candidate_title(&names);
        // コンポーネント名を短縮して1行に収める（先頭語のみ残す）
        let compact = names
            .iter()
            .map(|s| s.split('_').last().unwrap_or(s).to_string())
            .collect::<Vec<_>>()
            .join(" / ");
        writeln!(out, "  {BOLD}{}{RST}.  {title}  {DIM}[{n}要素]{RST}", i + 1).ok();
        writeln!(out, "     {DIM}{compact}{RST}").ok();
        if pairs.is_empty() {
            writeln!(out, "     {YLW}⚠ 依存関係未確定{RST}").ok();
        }
    }
    writeln!(out).ok();
    writeln!(out, "{DIM}  s <N> で選択  •  テキストを入力して追加要件{RST}").ok();
}

fn print_candidate_detail(
    requirement: &str,
    index: usize,
    candidate: &RankedCandidate,
    out: &mut impl Write,
) {
    let (names, pairs) = candidate_to_display_parts(candidate);
    let eval = &candidate.state.world_state.evaluation;
    let n = names.len();

    writeln!(out).ok();
    writeln!(out, "{BOLD}案 {index} — {}{RST}  {DIM}[{n}要素]{RST}", candidate_title(&names)).ok();
    writeln!(out, "{DIM}{}{RST}", "─".repeat(55)).ok();

    // コンポーネント一覧
    writeln!(out, "  {DIM}構成 :{RST}  {}", names.join("  /  ")).ok();

    // 依存関係
    if pairs.is_empty() {
        writeln!(out, "  {YLW}⚠ 依存関係が未確定{RST}  {DIM}— 編集バッファ・保存・描画更新の分離を追加で指定すると精度が上がります{RST}").ok();
    } else {
        let dep_str = pairs
            .iter()
            .map(|(f, t)| format!("{f} → {t}"))
            .collect::<Vec<_>>()
            .join("  |  ");
        writeln!(out, "  {DIM}依存 :{RST}  {dep_str}").ok();
    }
    writeln!(out).ok();

    // ─ スコア詳細（Design_BrainModel の推論評価値をそのまま表示）─
    writeln!(out, "  {DIM}スコア詳細  (Design_BrainModel 評価){RST}").ok();
    print_score_row(out, "構造品質      ", eval.structural_quality, false,
        "依存の整合性・レイヤー分離");
    print_score_row(out, "依存品質      ", eval.dependency_quality, false,
        "循環依存・結合度");
    print_score_row(out, "制約充足      ", eval.constraint_satisfaction, false,
        "要件制約の達成度");
    print_score_row(out, "複雑性        ", eval.complexity, true,
        "低いほど良い（シンプルさ）");
    print_score_row(out, "シミュレーション", eval.simulation_quality, false,
        "実行可能性推定");
    writeln!(out).ok();

    // ─ StructuredReasoningEngine による推論分析 ─
    let stability = eval.structural_quality * 0.6 + eval.dependency_quality * 0.4;
    let srt_input = StructuredReasoningInput {
        source_text: requirement.to_string(),
        selected_objective: requirement.lines().next().map(|l| l.trim().to_string()),
        requirement_count: names.len(),
        stability_score: stability,
        ambiguity_score: eval.complexity,
        evidence_spans: names.iter().take(3).cloned().collect(),
    };
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&srt_input);

    // 課題一覧
    let significant: Vec<_> = srt.issues.iter().filter(|i| i.severity >= 0.4).collect();
    if !significant.is_empty() {
        writeln!(out, "  {DIM}推論が検出した課題:{RST}").ok();
        for issue in significant.iter().take(3) {
            let color = if issue.severity >= 0.65 { YLW } else { DIM };
            let reason = issue.reason.as_deref().unwrap_or("不足あり");
            writeln!(out, "  {color}⚠ {reason}{RST}").ok();
        }
    }

    // 次に補強すべき軸
    writeln!(out, "  {DIM}→ 推奨アクション:{RST} {}",
        srt_next_action(srt.next_priority_axis)).ok();
    writeln!(out).ok();
}

fn score_bar(value: f64, width: usize) -> String {
    let filled = (value.clamp(0.0, 1.0) * width as f64).round() as usize;
    let empty = width - filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn print_score_row(out: &mut impl Write, label: &str, value: f64, invert: bool, desc: &str) {
    let display = if invert { 1.0 - value } else { value };
    let color = if display >= 0.7 { GRN } else if display >= 0.4 { YLW } else { RED };
    let bar = score_bar(display, 10);
    writeln!(out,
        "  {DIM}{label}{RST} {color}{bar}{RST} {:.2}  {DIM}{desc}{RST}",
        value
    ).ok();
}

fn srt_next_action(axis: ReasoningAxis) -> &'static str {
    match axis {
        ReasoningAxis::ProblemDefinition   => "解決課題と現状との差分を具体化してください",
        ReasoningAxis::TargetUser          => "対象ユーザーの属性と利用場面を明示してください",
        ReasoningAxis::ValueProposition    => "提供価値と既存との差別化を明文化してください",
        ReasoningAxis::SuccessMetric       => "成功指標を観測可能な条件で追加してください",
        ReasoningAxis::ScopeBoundary       => "含む範囲と含まない範囲を境界として定義してください",
        ReasoningAxis::Constraint          => "技術・予算・期間・法規制の制約を明記してください",
        ReasoningAxis::TechnicalStrategy   => "技術選定理由とアーキテクチャ方針を明確化してください",
        ReasoningAxis::RiskAssumption      => "主要な不確実性と外部依存を列挙してください",
    }
}

fn refine_session(sess: &mut SessionState, extra: &str, out: &mut impl Write) {
    let extra = extra.trim();
    if extra.is_empty() {
        writeln!(out, "{YLW}⚠ 追加したい要件を入力してください。{RST}").ok();
        return;
    }
    let combined = format!("{}\n{}", sess.requirement, extra);
    writeln!(out, "{DIM}追加要件を反映して設計案を更新します...{RST}").ok();
    let knowledge = match prepare_inference_input(&combined) {
        Ok(knowledge) => knowledge,
        Err(e) => {
            writeln!(out, "\nError: {e}").ok();
            return;
        }
    };
    let req = GenerateRequest::new(
        knowledge.enriched_requirement.clone(),
        10,
        5,
        3,
        true,
        false,
    );
    match run_phase9_pipeline(&req) {
        Ok(PipelineResult { ranked, .. }) => {
            print_candidates(&combined, &ranked, out);
            print_knowledge_context(out, &knowledge);
            sess.requirement = combined;
            sess.candidates = ranked;
            sess.selected = None;
        }
        Err(e) => {
            writeln!(out, "\n{RED}error:{RST} {e}").ok();
        }
    }
}

fn candidate_to_display_parts(c: &RankedCandidate) -> (Vec<String>, Vec<(String, String)>) {
    let architecture = arch_state_to_architecture(&c.state.architecture_state);
    let code_ir = DeterministicArchitectureToCodeIR::transform(&architecture);
    let names: Vec<String> = code_ir.modules.iter().map(|m| m.name.clone()).collect();
    let units_by_id = architecture.design_units_by_id();
    let pairs: Vec<(String, String)> = architecture
        .dependencies
        .iter()
        .filter_map(|dep| {
            let from = units_by_id.get(&dep.from.0).map(|u| u.name.clone())?;
            let to = units_by_id.get(&dep.to.0).map(|u| u.name.clone())?;
            Some((from, to))
        })
        .collect();
    (names, pairs)
}

fn build_saved_design(sess: &SessionState) -> SavedDesign {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let candidates = sess
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let eval = &c.state.world_state.evaluation;
            let (names, pairs) = candidate_to_display_parts(c);
            SavedCandidate {
                id: i + 1,
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
                dependencies: pairs.into_iter().map(|(f, t)| [f, t]).collect(),
                code_metrics: Default::default(),
            }
        })
        .collect();

    SavedDesign {
        version: "1.0".to_string(),
        generated_at: format!("{now}"),
        input: sess.requirement.clone(),
        search_states: 0,
        candidates,
    }
}

#[allow(dead_code)]
fn saved_candidate_from_ranked(id: usize, c: &RankedCandidate) -> SavedCandidate {
    let eval = &c.state.world_state.evaluation;
    let (names, pairs) = candidate_to_display_parts(c);
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
        dependencies: pairs.into_iter().map(|(f, t)| [f, t]).collect(),
        code_metrics: Default::default(),
    }
}

fn candidate_title(names: &[String]) -> &'static str {
    let service_count = names.iter().filter(|name| name.contains("service")).count();
    let controller_count = names
        .iter()
        .filter(|name| name.contains("controller"))
        .count();
    let repository_count = names
        .iter()
        .filter(|name| name.contains("repository"))
        .count();
    let database_count = names
        .iter()
        .filter(|name| name.contains("database"))
        .count();

    match (
        service_count,
        controller_count,
        repository_count,
        database_count,
    ) {
        (1, 1, 1, 1) => "単一編集コアでまとめた標準構成",
        (2.., 1, 1, 1) => "編集機能を分割した標準構成",
        (1, 2.., 1, 1) => "入力系を分割した標準構成",
        (2.., 2.., 1, 1) => "入力系と編集系を分割した拡張構成",
        (1.., 1.., 0, 0) => "永続化を持たない軽量構成",
        _ => "最小要素でまとめた構成",
    }
}

fn print_knowledge_context(
    out: &mut impl Write,
    knowledge: &crate::commands::knowledge_layer::InferenceKnowledgeContext,
) {
    if knowledge.temporary_hits.is_empty() && knowledge.grounding_hits.is_empty() {
        return;
    }

    writeln!(out, "{DIM}Knowledge Context").ok();
    if !knowledge.temporary_hits.is_empty() {
        writeln!(
            out,
            "  temporary: {} item(s)",
            knowledge.temporary_hits.len()
        )
        .ok();
    }
    if !knowledge.grounding_hits.is_empty() {
        writeln!(
            out,
            "  grounding: {} item(s)",
            knowledge.grounding_hits.len()
        )
        .ok();
    }
    writeln!(out, "{RST}").ok();
}

fn print_knowledge_status(out: &mut impl Write) {
    match knowledge_layer_metrics() {
        Ok(metrics) => {
            writeln!(out, "Knowledge layer status").ok();
            writeln!(
                out,
                "  temporary: {} item(s) [{}]",
                metrics.temporary_hits,
                temporary_knowledge_path().display()
            )
            .ok();
            match grounding_knowledge_path() {
                Ok(path) => {
                    writeln!(
                        out,
                        "  grounding: {} item(s) [{}]",
                        metrics.grounding_hits,
                        path.display()
                    )
                    .ok();
                }
                Err(e) => {
                    writeln!(out, "  grounding: {} item(s) [{e}]", metrics.grounding_hits).ok();
                }
            }
        }
        Err(e) => {
            writeln!(out, "Knowledge layer status unavailable: {e}").ok();
        }
    }
}

// ─── ディレクトリ表示ヘルパー ─────────────────────────────────────────────────

fn current_dir_display() -> String {
    std::env::current_dir()
        .map(|p| abbreviate_home(&p))
        .unwrap_or_else(|_| ".".to_string())
}

fn abbreviate_home(path: &std::path::Path) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(&home);
        if let Ok(rel) = path.strip_prefix(home_path) {
            let rel = rel.to_string_lossy();
            return if rel.is_empty() {
                "~".to_string()
            } else {
                format!("~/{rel}")
            };
        }
    }
    path.to_string_lossy().to_string()
}

// ─── スラッシュコマンドハンドラ ───────────────────────────────────────────────

/// インタラクティブセッション内でのスラッシュコマンドを処理する。
/// `/quit` など終了コマンドの場合は `true` を返す（ループ終了シグナル）。
fn handle_slash_interactive(
    input: &str,
    state: &mut Option<SessionState>,
    stdin: &io::Stdin,
    out: &mut impl Write,
) -> bool {
    let mut parts = input.splitn(2, ' ');
    let slash_cmd = parts.next().unwrap_or("");
    let slash_args = parts.next().unwrap_or("").trim();

    match slash_cmd {
        // ── 終了 ──
        "/q" | "/quit" | "/exit" => return true,

        // ── ヘルプ ──
        "/help" | "/h" => {
            print_help(out, state.is_some(), state.as_ref().and_then(|s| s.selected));
        }

        // ── 生成 ──
        "/generate" => {
            if slash_args.is_empty() {
                writeln!(out, "Usage: /generate <要件テキスト>").ok();
            } else {
                let requirement = slash_args.to_string();

                // ─ 推論ドリブンテンプレート補強ステップ ─
                let template = infer_template(&requirement);
                let enriched = {
                    let mut stdin_lock = stdin.lock();
                    let filled = match prompt_and_fill_dynamic(&template, &mut stdin_lock, out) {
                        Ok(f) => f,
                        Err(e) => {
                            writeln!(out, "\nError: {e}").ok();
                            return false;
                        }
                    };
                    enrich_dynamic(&requirement, &filled, &template)
                };

                write!(out, "Generating... ").ok();
                let _ = out.flush();
                let knowledge = match prepare_inference_input(&enriched.enriched_text) {
                    Ok(k) => k,
                    Err(e) => {
                        writeln!(out, "\nError: {e}").ok();
                        return false;
                    }
                };
                let req = GenerateRequest::new(
                    knowledge.enriched_requirement.clone(),
                    10 + enriched.beam_width_bonus,
                    5 + enriched.max_depth_bonus,
                    3,
                    true,
                    false,
                );
                match run_phase9_pipeline(&req) {
                    Ok(PipelineResult { ranked, search_states_count }) => {
                        writeln!(out, "done ({search_states_count} states searched)").ok();
                        writeln!(out).ok();
                        print_candidates(&enriched.enriched_text, &ranked, out);
                        print_knowledge_context(out, &knowledge);
                        *state = Some(SessionState {
                            requirement: enriched.enriched_text,
                            candidates: ranked,
                            selected: None,
                            latest_web_hits: Vec::new(),
                        });
                    }
                    Err(e) => {
                        writeln!(out, "\nError: {e}").ok();
                    }
                }
            }
        }

        // ── 候補一覧 ──
        "/list" | "/ls" => {
            if let Some(ref sess) = *state {
                print_candidates(&sess.requirement, &sess.candidates, out);
            } else {
                writeln!(out, "セッション未開始。/generate <要件> で開始してください。").ok();
            }
        }

        // ── 候補選択 ──
        "/s" | "/select" => {
            if let Some(ref mut sess) = *state {
                let idx: usize = slash_args.parse().unwrap_or(1);
                if idx == 0 || idx > sess.candidates.len() {
                    writeln!(out, "Invalid index. Choose 1–{}", sess.candidates.len()).ok();
                } else {
                    sess.selected = Some(idx - 1);
                    writeln!(out, "案 {idx} を選択しました。").ok();
                    if let Some(candidate) = sess.candidates.get(idx - 1) {
                        print_candidate_detail(&sess.requirement, idx, candidate, out);
                    }
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── Mermaid ──
        "/m" | "/mermaid" => {
            if let Some(ref sess) = *state {
                let idx = sess.selected.unwrap_or(0);
                if let Some(c) = sess.candidates.get(idx) {
                    let (names, pairs) = candidate_to_display_parts(c);
                    writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                } else {
                    writeln!(out, "No candidate selected. Use /select <N> first.").ok();
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── エクスポート ──
        "/e" | "/export" => {
            if let Some(ref sess) = *state {
                let fmt = if slash_args.is_empty() { "text" } else { slash_args };
                let idx = sess.selected.unwrap_or(0);
                if let Some(c) = sess.candidates.get(idx) {
                    let (names, pairs) = candidate_to_display_parts(c);
                    match fmt {
                        "mermaid" => {
                            writeln!(out, "{}", build_mermaid(&names, &pairs)).ok();
                        }
                        "markdown" => {
                            let ev = c.state.world_state.evaluation.clone();
                            let display = CandidateDisplay {
                                score: c.score,
                                pareto_rank: c.pareto_rank,
                                component_names: names,
                                dependency_pairs: pairs,
                                evaluation: ev,
                                generated_files: vec![],
                            };
                            writeln!(
                                out,
                                "{}",
                                build_markdown(&sess.requirement, 0, &[display])
                            )
                            .ok();
                        }
                        _ => {
                            print_candidate_detail(&sess.requirement, idx + 1, c, out);
                        }
                    }
                } else {
                    writeln!(out, "No candidate selected.").ok();
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── リファイン ──
        "/r" | "/refine" => {
            if let Some(ref mut sess) = *state {
                if slash_args.is_empty() {
                    write!(out, "Additional requirement: ").ok();
                    let _ = out.flush();
                    let mut extra = String::new();
                    stdin.lock().read_line(&mut extra).ok();
                    let extra = extra.trim().to_string();
                    refine_session(sess, &extra, out);
                } else {
                    refine_session(sess, slash_args, out);
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── 保存 ──
        "/save" => {
            if let Some(ref sess) = *state {
                let arg = if slash_args.is_empty() { "design_session.json" } else { slash_args };
                let saved = build_saved_design(sess);
                if arg.contains('/') || arg.ends_with(".json") {
                    if let Err(e) = save_design_file(&saved, Path::new(arg)) {
                        writeln!(out, "Error: {e}").ok();
                    } else {
                        writeln!(out, "Saved to {arg}").ok();
                    }
                } else {
                    let store = DesignStore::new();
                    match store.save(arg, &saved) {
                        Ok(path) => {
                            writeln!(out, "Saved as '{}' → {}", arg, path.display()).ok();
                        }
                        Err(e) => {
                            writeln!(out, "Error: {e}").ok();
                        }
                    }
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── ストア一覧 ──
        "/saves" | "/store" => {
            let store = DesignStore::new();
            match store.list() {
                Ok(entries) => {
                    writeln!(out, "{}", format_store_list(&entries)).ok();
                }
                Err(e) => {
                    writeln!(out, "Error: {e}").ok();
                }
            }
        }

        // ── 読み込み ──
        "/load" => {
            if slash_args.is_empty() {
                writeln!(out, "Usage: /load <name>").ok();
            } else {
                let store = DesignStore::new();
                match store.load(slash_args) {
                    Ok(design) => {
                        let req = GenerateRequest::new(
                            design.input.clone(),
                            10,
                            5,
                            design.candidates.len().max(1),
                            true,
                            false,
                        );
                        match run_phase9_pipeline(&req) {
                            Ok(PipelineResult { ranked, .. }) => {
                                let requirement = design.input.clone();
                                writeln!(
                                    out,
                                    "Loaded '{}' — {} candidates.",
                                    slash_args,
                                    ranked.len()
                                )
                                .ok();
                                print_candidates(&requirement, &ranked, out);
                                *state = Some(SessionState {
                                    requirement,
                                    candidates: ranked,
                                    selected: None,
                                    latest_web_hits: Vec::new(),
                                });
                            }
                            Err(e) => {
                                writeln!(out, "Error restoring pipeline: {e}").ok();
                            }
                        }
                    }
                    Err(e) => {
                        writeln!(out, "Error: {e}").ok();
                    }
                }
            }
        }

        // ── チャット ──
        "/chat" => {
            if let Some(ref sess) = *state {
                run_chat_session(
                    &sess.requirement,
                    &sess.candidates,
                    sess.selected,
                    stdin,
                    out,
                );
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── Web 検索 ──
        "/w" | "/web-search" => {
            if let Some(ref mut sess) = *state {
                let query = if slash_args.is_empty() {
                    sess.requirement.clone()
                } else {
                    slash_args.to_string()
                };
                match crate::commands::web_search::search(&query, 5) {
                    Ok(hits) if hits.is_empty() => {
                        writeln!(out, "Web search returned no results.").ok();
                    }
                    Ok(hits) => {
                        match save_temporary_web_hits(&query, &hits) {
                            Ok(saved_hits) => {
                                sess.latest_web_hits = saved_hits.clone();
                            }
                            Err(e) => {
                                writeln!(out, "Failed to save temporary knowledge: {e}").ok();
                                sess.latest_web_hits.clear();
                            }
                        }
                        writeln!(out, "Web knowledge for: {query}").ok();
                        for (i, hit) in sess.latest_web_hits.iter().enumerate() {
                            writeln!(out, "  {}. {}", i + 1, hit.title).ok();
                            writeln!(out, "     {}", hit.snippet).ok();
                        }
                        if !sess.latest_web_hits.is_empty() {
                            writeln!(
                                out,
                                "Temporary knowledge saved to {}",
                                temporary_knowledge_path().display()
                            )
                            .ok();
                        }
                    }
                    Err(e) => {
                        writeln!(out, "Web search error: {e}").ok();
                    }
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── Grounding 昇格 ──
        "/g" | "/ground" => {
            if let Some(ref mut sess) = *state {
                let idx: usize = slash_args.parse().unwrap_or(1);
                if idx == 0 || idx > sess.latest_web_hits.len() {
                    writeln!(out, "Invalid index. Choose 1–{}", sess.latest_web_hits.len()).ok();
                } else {
                    let hit_id = sess.latest_web_hits[idx - 1].id;
                    match promote_hits_to_grounding(&[hit_id]) {
                        Ok(promoted) if promoted.is_empty() => {
                            writeln!(out, "Selected knowledge was not promoted.").ok();
                        }
                        Ok(promoted) => {
                            writeln!(
                                out,
                                "Promoted {} item(s) to grounding knowledge.",
                                promoted.len()
                            )
                            .ok();
                            sess.latest_web_hits.retain(|hit| hit.id != hit_id);
                        }
                        Err(e) => {
                            writeln!(out, "Grounding promotion error: {e}").ok();
                        }
                    }
                }
            } else {
                writeln!(out, "セッション未開始。").ok();
            }
        }

        // ── 知識ステータス ──
        "/k" | "/knowledge" => {
            print_knowledge_status(out);
        }

        // ── 知識監査 ──
        "/knowledge-audit" => {
            let _ = crate::commands::knowledge_audit::run("text");
        }

        // ── スキャン ──
        "/scan" => {
            if slash_args.is_empty() {
                writeln!(out, "Usage: /scan <ディレクトリ>").ok();
            } else {
                let _ = crate::commands::scan::run(crate::commands::scan::ScanArgs {
                    dir: slash_args.to_string(),
                    format: "text".to_string(),
                    output: None,
                    depth: 3,
                    include: "**/*.rs".to_string(),
                    verbose: false,
                });
            }
        }

        // ── 評価 ──
        "/evaluate" => {
            if slash_args.is_empty() {
                writeln!(out, "Usage: /evaluate <design_file>").ok();
            } else {
                let _ = crate::commands::evaluate::run(slash_args);
            }
        }

        // ── 説明 ──
        "/explain" => {
            if slash_args.is_empty() {
                writeln!(out, "Usage: /explain <design_file>").ok();
            } else {
                let _ = crate::commands::explain::run(slash_args);
            }
        }

        // ── すでにインタラクティブ ──
        "/i" | "/interactive" => {
            writeln!(out, "すでにインタラクティブモードです。").ok();
        }

        // ── 未知のコマンド ──
        _ => {
            writeln!(
                out,
                "Unknown command: {slash_cmd}  (/help でコマンド一覧を表示)"
            )
            .ok();
        }
    }

    false // 終了しない
}

#[cfg(test)]
mod tests {
    use super::*;

    /// パイプで入力を渡してインタラクティブモードをテスト
    #[test]
    fn test_interactive_quit_immediately() {
        // stdin に "q\n" を渡す → 即座に終了
        // run() は stdin を直接使うため、ここでは引数のバリデーションのみテスト
        let args = InteractiveArgs { from: None };
        // from=None の場合は常に Ok で返る（ループ起動まで）
        // 実際の REPL は stdin に依存するため統合テストで検証
        drop(args); // コンパイル確認のみ
    }

    #[test]
    fn test_interactive_invalid_from_file_is_warned_not_errored() {
        // --from に存在しないファイルを渡しても Warning のみで続行できる設計
        // (run() 内で Err をログするだけ)
        // ここでは load_design_file のエラーパスが適切に処理されることを確認
        let result = crate::input_bridge::load_design_file(Path::new("/nonexistent.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_requirement_input_strips_cli_prefix() {
        assert_eq!(
            normalize_requirement_input(r#"arch_gen /generate "NeoVim風のEditorを作成したい""#),
            "NeoVim風のEditorを作成したい"
        );
        assert_eq!(
            normalize_requirement_input("/generate NeoVim風のEditorを作成したい"),
            "NeoVim風のEditorを作成したい"
        );
        assert_eq!(
            normalize_requirement_input("generate 'NeoVim風のEditorを作成したい'"),
            "NeoVim風のEditorを作成したい"
        );
    }

    #[test]
    fn test_normalize_requirement_input_keeps_plain_text() {
        assert_eq!(
            normalize_requirement_input("ターミナルUIとモード切替を持つこと"),
            "ターミナルUIとモード切替を持つこと"
        );
    }
}
