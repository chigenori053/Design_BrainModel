use design_reasoning::LanguageState;

use crate::input_bridge::SavedCandidate;

/// `SavedCandidate` の評価スコアを `LanguageEngine` で自然言語化する。
pub fn verbalize_candidate(input: &str, c: &SavedCandidate) -> String {
    let state = build_language_state(input, c);

    let mut out = String::new();
    out.push_str("この案の見立て:\n");
    if let Some(objective) = &state.selected_objective {
        out.push_str(&format!("{objective} を実現するための初期案です。\n"));
    } else {
        out.push_str("要求を形にするための初期案です。\n");
    }
    let template_desc = select_template_description(state.stability_score, state.ambiguity_score);
    out.push_str("設計上のコメント:\n");
    out.push_str(template_desc);

    out
}

/// `LanguageState` を評価スコアから構築する。
fn build_language_state(input: &str, c: &SavedCandidate) -> LanguageState {
    // 設計目標: 入力テキストの先頭行（最大60文字）
    let objective = input.lines().find(|l| !l.trim().is_empty()).map(|l| {
        let s = l.trim();
        if s.chars().count() > 60 {
            let truncated: String = s.chars().take(57).collect();
            format!("{truncated}...")
        } else {
            s.to_string()
        }
    });

    // 安定性スコア: structural_quality と dependency_quality の加重平均
    let stability_score = (c.evaluation.structural_quality * 0.6
        + c.evaluation.dependency_quality * 0.4)
        .clamp(0.0, 1.0);

    // 曖昧性スコア: complexity を曖昧性の代理指標として使用（高複雑 → 高曖昧）
    let ambiguity_score = c.evaluation.complexity.clamp(0.0, 1.0);

    LanguageState {
        selected_objective: objective,
        requirement_count: c.components.len(),
        stability_score,
        ambiguity_score,
    }
}

/// stability/ambiguity スコアからテンプレート説明文を選択する。
fn select_template_description(stability: f64, ambiguity: f64) -> &'static str {
    match (stability >= 0.7, ambiguity < 0.5) {
        (true, true) => {
            "設計構造は極めて安定しており、意図が明確に反映されています。このまま実装または詳細設計へ進むことが可能です。"
        }
        (true, false) => {
            "構造的な安定性は確保されていますが、一部の要件に曖昧さが残っています。特に用語の定義や制約条件の具体化を検討してください。"
        }
        (false, true) => {
            "意図は明確ですが、設計構造に不安定な箇所が見られます。要件間の競合や複雑性が増大している可能性があるため、構造の再構成を検討してください。"
        }
        (false, false) => {
            "設計は極めて不安定で、かつ意図も不明確な状態です。核となる設計目標を再定義し、スモールステップでの分析を推奨します。"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{SavedCandidate, SavedCodeMetrics, SavedEvaluation};

    fn make_candidate(structural: f64, dependency: f64, complexity: f64) -> SavedCandidate {
        SavedCandidate {
            id: 1,
            score: 0.8,
            pareto_rank: 0,
            evaluation: SavedEvaluation {
                structural_quality: structural,
                dependency_quality: dependency,
                constraint_satisfaction: 0.9,
                complexity,
                simulation_quality: 0.9,
                total: 0.8,
            },
            components: vec![
                "service_1".to_string(),
                "service_2".to_string(),
                "database_1".to_string(),
            ],
            dependencies: vec![["service_1".to_string(), "database_1".to_string()]],
            code_metrics: SavedCodeMetrics::default(),
        }
    }

    #[test]
    fn test_verbalize_returns_nonempty() {
        let c = make_candidate(0.9, 0.85, 0.2);
        let result = verbalize_candidate("ECサイトをスケーラブルに設計したい", &c);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_verbalize_stable_clear_contains_stability_description() {
        let c = make_candidate(0.95, 0.9, 0.2); // 高安定・低複雑
        let result = verbalize_candidate("Design a stable system", &c);
        assert!(result.contains("安定"), "should mention stability");
    }

    #[test]
    fn test_verbalize_unstable_contains_warning() {
        let c = make_candidate(0.3, 0.3, 0.8); // 低安定・高複雑
        let result = verbalize_candidate("Complex system", &c);
        assert!(
            result.contains("不安定") || result.contains("再定義"),
            "should warn about instability"
        );
    }

    #[test]
    fn test_build_language_state_objective_truncated() {
        let long_input = "a".repeat(100);
        let c = make_candidate(0.8, 0.7, 0.3);
        let state = build_language_state(&long_input, &c);
        let obj = state.selected_objective.unwrap();
        assert!(obj.ends_with("..."), "long input should be truncated");
        assert!(obj.chars().count() <= 60);
    }

    #[test]
    fn test_build_language_state_scores() {
        let c = make_candidate(1.0, 1.0, 0.0);
        let state = build_language_state("test", &c);
        assert!((state.stability_score - 1.0).abs() < 0.01);
        assert!((state.ambiguity_score - 0.0).abs() < 0.01);
    }
}
