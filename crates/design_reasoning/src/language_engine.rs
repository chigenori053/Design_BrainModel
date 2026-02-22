use semantic_dhm::{DesignProjection, SemanticUnitL1};

use crate::DesignHypothesis;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub summary: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageState {
    pub selected_objective: Option<String>,
    pub requirement_count: usize,
    pub stability_score: f64,
    pub ambiguity_score: f64,
}

#[derive(Clone, Default)]
pub struct LanguageEngine;

impl LanguageEngine {
    pub fn build_state(
        &self,
        projection: &DesignProjection,
        l1_units: &[SemanticUnitL1],
        hypothesis: &DesignHypothesis,
    ) -> LanguageState {
        let selected_objective = l1_units
            .iter()
            .find(|u| !u.source_text.trim().is_empty())
            .map(|u| u.source_text.clone());

        // L1の抽象度が高いほど曖昧とみなす簡易指標（0..1）
        let ambiguity_score = if l1_units.is_empty() {
            1.0
        } else {
            let mean_abs = l1_units.iter().map(|u| f64::from(u.abstraction)).sum::<f64>()
                / l1_units.len() as f64;
            mean_abs.clamp(0.0, 1.0)
        };

        // 制約違反があると安定度を低下させる決定論的スコア
        let penalty = if hypothesis.constraint_violation { 0.25 } else { 0.0 };
        let stability_score = (1.0 - hypothesis.normalized_score.abs() * 0.2 - penalty).clamp(0.0, 1.0);

        LanguageState {
            selected_objective,
            requirement_count: projection.derived.len(),
            stability_score,
            ambiguity_score,
        }
    }

    pub fn explain_state(&self, state: &LanguageState) -> Explanation {
        let objective = state
            .selected_objective
            .as_deref()
            .unwrap_or("未指定");
        let stability_label = if state.stability_score > 0.85 {
            "安定"
        } else if state.stability_score >= 0.6 {
            "概ね安定"
        } else {
            "不安定"
        };
        let ambiguity_label = if state.ambiguity_score > 0.7 {
            "不明確"
        } else if state.ambiguity_score >= 0.4 {
            "部分的に不明確"
        } else {
            "明確"
        };

        let summary = format!(
            "設計目標: {objective}\n派生要件数: {}\n構造安定性: {stability_label}\n曖昧性: {ambiguity_label}",
            state.requirement_count
        );
        let detail = format!(
            "stability_score={:.3}, ambiguity_score={:.3}",
            state.stability_score, state.ambiguity_score
        );
        Explanation { summary, detail }
    }

    pub fn explain(&self, hypothesis: &DesignHypothesis) -> Explanation {
        let state = LanguageState {
            selected_objective: None,
            requirement_count: hypothesis.requirements.len(),
            stability_score: (1.0 - hypothesis.normalized_score.abs() * 0.2).clamp(0.0, 1.0),
            ambiguity_score: 1.0,
        };
        self.explain_state(&state)
    }
}
