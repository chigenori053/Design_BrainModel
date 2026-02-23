use serde::{Deserialize, Serialize};
use language_dhm::{EMBEDDING_DIM, LangId, LanguageDhm, LanguageUnit};
use semantic_dhm::{DesignProjection, SemanticError, SemanticUnitL1};

use crate::DesignHypothesis;

pub const TEMPLATE_SELECTION_EPSILON: f32 = 1e-6;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    pub summary: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanguageState {
    pub selected_objective: Option<String>,
    pub requirement_count: usize,
    pub stability_score: f64,
    pub ambiguity_score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanguageStateV2 {
    pub selected_objective: Option<String>,
    pub requirement_count: usize,
    pub stability_score: f64,
    pub ambiguity_score: f64,
}

impl From<LanguageState> for LanguageStateV2 {
    fn from(value: LanguageState) -> Self {
        Self {
            selected_objective: value.selected_objective,
            requirement_count: value.requirement_count,
            stability_score: value.stability_score.clamp(0.0, 1.0),
            ambiguity_score: value.ambiguity_score.clamp(0.0, 1.0),
        }
    }
}

impl From<&LanguageState> for LanguageStateV2 {
    fn from(value: &LanguageState) -> Self {
        Self {
            selected_objective: value.selected_objective.clone(),
            requirement_count: value.requirement_count,
            stability_score: value.stability_score.clamp(0.0, 1.0),
            ambiguity_score: value.ambiguity_score.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateId {
    StableClear,
    StableAmbiguous,
    UnstableClear,
    UnstableAmbiguous,
    Fallback,
}

impl TemplateId {
    pub fn as_label(self) -> &'static str {
        match self {
            TemplateId::StableClear => "stable_clear",
            TemplateId::StableAmbiguous => "stable_ambiguous",
            TemplateId::UnstableClear => "unstable_clear",
            TemplateId::UnstableAmbiguous => "unstable_ambiguous",
            TemplateId::Fallback => "fallback",
        }
    }

    pub fn as_description(self) -> &'static str {
        match self {
            TemplateId::StableClear => "設計構造は極めて安定しており、意図が明確に反映されています。このまま実装または詳細設計へ進むことが可能です。",
            TemplateId::StableAmbiguous => "構造的な安定性は確保されていますが、一部の要件に曖昧さが残っています。特に用語の定義や制約条件の具体化を検討してください。",
            TemplateId::UnstableClear => "意図は明確ですが、設計構造に不安定な箇所が見られます。要件間の競合や複雑性が増大している可能性があるため、構造の再構成を検討してください。",
            TemplateId::UnstableAmbiguous => "設計は極めて不安定で、かつ意図も不明確な状態です。核となる設計目標を再定義し、スモールステップでの分析を推奨します。",
            TemplateId::Fallback => "分析結果から十分な傾向を読み取れませんでした。追加の要件を入力して分析を継続してください。",
        }
    }
}

pub struct LanguagePatternStore {
    dhm: LanguageDhm<memory_store::InMemoryStore<LangId, LanguageUnit>>,
    mapping: std::collections::BTreeMap<LangId, TemplateId>,
}

impl LanguagePatternStore {
    pub fn new() -> Result<Self, SemanticError> {
        let mut dhm = LanguageDhm::in_memory().map_err(|e| SemanticError::EvaluationError(e.to_string()))?;
        let mut mapping = std::collections::BTreeMap::new();
        for (template, vec) in [
            (TemplateId::StableClear, vec![1.0, 0.0, 0.2, 1.0]),
            (TemplateId::StableAmbiguous, vec![1.0, 1.0, 0.2, 1.0]),
            (TemplateId::UnstableClear, vec![0.0, 0.0, 0.8, 1.0]),
            (TemplateId::UnstableAmbiguous, vec![0.0, 1.0, 0.8, 1.0]),
            (TemplateId::Fallback, vec![0.5, 0.5, 0.5, 0.0]),
        ] {
            let mut emb = vec![0.0f32; EMBEDDING_DIM];
            for (idx, value) in vec.into_iter().enumerate() {
                emb[idx] = value;
            }
            let id = dhm
                .insert(template.as_label(), emb)
                .map_err(|e| SemanticError::EvaluationError(e.to_string()))?;
            mapping.insert(id, template);
        }
        Ok(Self { dhm, mapping })
    }

    pub fn select_template(&self, h: &[f32]) -> Result<TemplateId, SemanticError> {
        let top = self.dhm.recall(h, 3);
        if top.is_empty() {
            return Ok(TemplateId::Fallback);
        }
        let top1 = top[0];
        let top2 = top.get(1).copied();
        let first = self
            .mapping
            .get(&top1.0)
            .copied()
            .ok_or(SemanticError::InconsistentState("template mapping missing"))?;
        if let Some(second) = top2 {
            let margin = (top1.1 - second.1).abs();
            if is_ambiguous_margin(margin) {
                return Ok(TemplateId::Fallback);
            }
        }
        Ok(first)
    }
}

pub struct LanguageEngine {
    patterns: Option<LanguagePatternStore>,
}

impl Default for LanguageEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageEngine {
    pub fn new() -> Self {
        Self {
            patterns: LanguagePatternStore::new().ok(),
        }
    }
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
        let state_v2 = LanguageStateV2::from(state);
        let h = self.build_h_state(&state_v2);
        let template = self
            .select_template(&h)
            .unwrap_or(TemplateId::Fallback);
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
            "template={}, stability_score={:.3}, ambiguity_score={:.3}",
            template.as_label(),
            state.stability_score,
            state.ambiguity_score
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

    pub fn build_h_state(&self, state: &LanguageStateV2) -> Vec<f32> {
        let mut h = vec![0.0f32; EMBEDDING_DIM];
        let requirement_count_norm = (state.requirement_count as f64 / 16.0).clamp(0.0, 1.0);
        h[0] = state.stability_score.clamp(0.0, 1.0) as f32;
        h[1] = state.ambiguity_score.clamp(0.0, 1.0) as f32;
        h[2] = requirement_count_norm as f32;
        h[3] = if state.selected_objective.is_some() { 1.0 } else { 0.0 };
        h
    }

    pub fn select_template(&self, h_state: &[f32]) -> Result<TemplateId, SemanticError> {
        let Some(patterns) = &self.patterns else {
            return Ok(TemplateId::Fallback);
        };
        patterns.select_template(h_state)
    }
}

pub fn is_ambiguous_margin(margin: f32) -> bool {
    margin <= TEMPLATE_SELECTION_EPSILON
}
