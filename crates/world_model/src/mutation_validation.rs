use crate::semantic_causal_runtime::SemanticRuntimeError;
use crate::{
    CausalPropagationGraph, CausalRuntimeState, CausalStability, EnvironmentSync,
    SemanticCausalEngine, WorldStateToCausalAdapter,
};
use world_model_core::WorldState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalStabilityLevel {
    Stable,
    Degrading,
    Unstable,
    Contradictory,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionResult {
    pub stability: CausalStability,
    pub consequence_score: f64,
    pub affected_entities: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionExplanation {
    pub summary: String,
    pub primary_causes: Vec<PredictionCause>,
    pub affected_entities: Vec<AffectedEntity>,
    pub recommendations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredictionCause {
    pub category: CauseCategory,
    pub severity: CauseSeverity,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CauseCategory {
    DependencyConcentration,
    ResponsibilityDrift,
    BoundaryViolation,
    StructuralInstability,
    ConstraintViolation,
    CausalConflict,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CauseSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AffectedEntity {
    pub name: String,
    pub impact_score: f64,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationDecision {
    Allow,
    Warn,
    Reject,
}

pub struct MutationValidationGate;
pub struct PredictionExplainer;
pub struct RecommendationGenerator;

impl MutationValidationGate {
    pub fn predict(state: &WorldState) -> PredictionResult {
        let causal_state = WorldStateToCausalAdapter::convert(state);
        Self::predict_causal(&causal_state)
    }

    pub fn decide(prediction: &PredictionResult) -> ValidationDecision {
        match Self::classify(&prediction.stability) {
            CausalStabilityLevel::Stable => ValidationDecision::Allow,
            CausalStabilityLevel::Degrading => ValidationDecision::Warn,
            CausalStabilityLevel::Unstable | CausalStabilityLevel::Contradictory => {
                ValidationDecision::Reject
            }
        }
    }

    pub fn classify(stability: &CausalStability) -> CausalStabilityLevel {
        if stability.world_consistency < 0.5 && stability.causal_entropy >= 1.0 {
            CausalStabilityLevel::Contradictory
        } else if stability.requires_halt()
            || stability.future_divergence >= 0.66
            || stability.world_consistency < 0.5
        {
            CausalStabilityLevel::Unstable
        } else if stability.future_divergence >= 0.33
            || stability.causal_entropy >= 0.5
            || stability.world_consistency < 0.8
            || stability.semantic_drift >= 0.5
        {
            CausalStabilityLevel::Degrading
        } else {
            CausalStabilityLevel::Stable
        }
    }

    pub fn narrative(decision: ValidationDecision, explanation: &PredictionExplanation) -> String {
        let mut sections = vec![
            "変更内容を評価しました。".to_string(),
            explanation.summary.clone(),
        ];

        if !explanation.primary_causes.is_empty() {
            sections.push(format!(
                "主な原因\n{}",
                explanation
                    .primary_causes
                    .iter()
                    .map(|cause| format!("・{}", cause.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !explanation.affected_entities.is_empty() {
            sections.push(format!(
                "影響が予測される領域\n{}",
                explanation
                    .affected_entities
                    .iter()
                    .enumerate()
                    .map(|(index, entity)| format!(
                        "{}. {} (impact: {:.2})",
                        index + 1,
                        entity.name,
                        entity.impact_score
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !explanation.recommendations.is_empty() {
            sections.push(format!(
                "推奨事項\n{}",
                explanation
                    .recommendations
                    .iter()
                    .map(|recommendation| format!("・{recommendation}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        sections.push(
            match decision {
                ValidationDecision::Allow => "変更を適用できます。",
                ValidationDecision::Warn => "適用は可能ですが、影響範囲の確認を推奨します。",
                ValidationDecision::Reject => "変更は拒否されました。",
            }
            .to_string(),
        );
        sections.join("\n\n")
    }

    fn predict_causal(state: &CausalRuntimeState) -> PredictionResult {
        let graph = CausalPropagationGraph {
            edges: state.causal_state.edges.clone(),
        };
        let engine = SemanticCausalEngine::new(graph);
        let sync = EnvironmentSync::synchronized(state, 0);
        let action = state
            .causal_state
            .edges
            .first()
            .map(|edge| edge.source_state.as_str())
            .unwrap_or("");

        match engine.predict(state, action, &state.world_signature, &sync) {
            Ok(simulation) => {
                let mut affected_entities = state
                    .entities
                    .iter()
                    .zip(&simulation.projected_world_state.entities)
                    .filter(|(before, after)| before.current_state != after.current_state)
                    .map(|(before, _)| before.entity_id.clone())
                    .collect::<Vec<_>>();
                affected_entities.sort();
                affected_entities.dedup();

                let stability = simulation.causal_stability;
                PredictionResult {
                    stability,
                    consequence_score: consequence_score(&stability),
                    affected_entities,
                    warnings: warnings_for(&stability),
                }
            }
            Err(SemanticRuntimeError::FutureInstabilityOverflow { stability }) => {
                let mut affected_entities = state
                    .causal_state
                    .edges
                    .iter()
                    .flat_map(|edge| [edge.source_state.clone(), edge.target_state.clone()])
                    .collect::<Vec<_>>();
                affected_entities.sort();
                affected_entities.dedup();

                PredictionResult {
                    stability,
                    consequence_score: consequence_score(&stability),
                    affected_entities,
                    warnings: warnings_for(&stability),
                }
            }
            Err(SemanticRuntimeError::WorldStateStale) => unreachable!(
                "the validation gate creates synchronization from the same causal state"
            ),
        }
    }
}

impl PredictionExplainer {
    pub fn explain(prediction: &PredictionResult) -> PredictionExplanation {
        let level = MutationValidationGate::classify(&prediction.stability);
        let primary_causes = extract_causes(prediction, level);
        let affected_entities = rank_affected_entities(prediction, &primary_causes);
        let mut explanation = PredictionExplanation {
            summary: summary_for(level).to_string(),
            primary_causes,
            affected_entities,
            recommendations: Vec::new(),
        };
        explanation.recommendations = RecommendationGenerator::generate(&explanation);
        explanation
    }
}

impl RecommendationGenerator {
    pub fn generate(explanation: &PredictionExplanation) -> Vec<String> {
        let mut recommendations = explanation
            .primary_causes
            .iter()
            .map(|cause| recommendation_for(cause.category))
            .collect::<Vec<_>>();
        recommendations.sort();
        recommendations.dedup();
        recommendations
    }
}

fn extract_causes(
    prediction: &PredictionResult,
    level: CausalStabilityLevel,
) -> Vec<PredictionCause> {
    let stability = &prediction.stability;
    let mut causes = Vec::new();

    if level == CausalStabilityLevel::Contradictory {
        causes.push(PredictionCause {
            category: CauseCategory::CausalConflict,
            severity: CauseSeverity::Critical,
            description: "矛盾する構造状態が検出されました。".to_string(),
        });
    }
    if stability.causal_entropy >= 0.5 && prediction.affected_entities.len() >= 2 {
        causes.push(PredictionCause {
            category: CauseCategory::DependencyConcentration,
            severity: severity_for(stability.causal_entropy),
            description: "依存関係の集中が予測されます。".to_string(),
        });
    }
    if stability.semantic_drift >= 0.5 {
        causes.push(PredictionCause {
            category: CauseCategory::ResponsibilityDrift,
            severity: severity_for(stability.semantic_drift),
            description: "責務境界の曖昧化が予測されます。".to_string(),
        });
    }
    if stability.world_consistency < 0.8 {
        causes.push(PredictionCause {
            category: CauseCategory::BoundaryViolation,
            severity: severity_for(1.0 - stability.world_consistency),
            description: "レイヤ境界の逸脱が予測されます。".to_string(),
        });
    }
    if stability.future_divergence >= 0.33 || stability.causal_entropy >= 0.5 {
        causes.push(PredictionCause {
            category: CauseCategory::StructuralInstability,
            severity: severity_for(stability.future_divergence.max(stability.causal_entropy)),
            description: "構造安定性の低下が予測されます。".to_string(),
        });
    }
    if level != CausalStabilityLevel::Stable && causes.is_empty() {
        causes.push(PredictionCause {
            category: CauseCategory::Unknown,
            severity: CauseSeverity::Medium,
            description: "複合的な構造リスクが予測されます。".to_string(),
        });
    }

    causes.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then(left.category.cmp(&right.category))
            .then(left.description.cmp(&right.description))
    });
    causes.dedup_by(|left, right| left.category == right.category);
    causes
}

fn rank_affected_entities(
    prediction: &PredictionResult,
    causes: &[PredictionCause],
) -> Vec<AffectedEntity> {
    let mut names = prediction.affected_entities.clone();
    names.sort();
    names.dedup();

    let base_impact = (1.0 - prediction.consequence_score).clamp(0.0, 1.0);
    let reason = causes
        .first()
        .map(|cause| cause.description.clone())
        .unwrap_or_else(|| "予測された構造変更の影響範囲です。".to_string());
    let severity_impact = causes
        .first()
        .map(|cause| match cause.severity {
            CauseSeverity::Low => 0.25,
            CauseSeverity::Medium => 0.5,
            CauseSeverity::High => 0.75,
            CauseSeverity::Critical => 1.0,
        })
        .unwrap_or_default();
    let impact_score = base_impact.max(severity_impact);
    let mut entities = names
        .into_iter()
        .map(|name| AffectedEntity {
            name,
            impact_score,
            reason: reason.clone(),
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .impact_score
            .total_cmp(&left.impact_score)
            .then(left.name.cmp(&right.name))
    });
    entities
}

fn summary_for(level: CausalStabilityLevel) -> &'static str {
    match level {
        CausalStabilityLevel::Stable => "構造安定性は維持されています。",
        CausalStabilityLevel::Degrading => "構造安定性の低下が予測されます。",
        CausalStabilityLevel::Unstable => "構造不安定化が予測されました。",
        CausalStabilityLevel::Contradictory => "矛盾する因果状態が予測されました。",
    }
}

fn severity_for(value: f64) -> CauseSeverity {
    if value >= 1.0 {
        CauseSeverity::Critical
    } else if value >= 0.66 {
        CauseSeverity::High
    } else if value >= 0.33 {
        CauseSeverity::Medium
    } else {
        CauseSeverity::Low
    }
}

fn recommendation_for(category: CauseCategory) -> String {
    match category {
        CauseCategory::DependencyConcentration => "依存関係を分散することを推奨します。",
        CauseCategory::ResponsibilityDrift => "責務境界を見直してください。",
        CauseCategory::BoundaryViolation => "レイヤ境界を再確認してください。",
        CauseCategory::StructuralInstability => "変更を小さな単位に分割してください。",
        CauseCategory::ConstraintViolation => "構造制約と変更内容を再確認してください。",
        CauseCategory::CausalConflict => "矛盾する依存関係を解消してください。",
        CauseCategory::Unknown => "影響範囲を確認し、構造変更を再評価してください。",
    }
    .to_string()
}

fn consequence_score(stability: &CausalStability) -> f64 {
    let risk = (stability.causal_entropy
        + stability.future_divergence
        + (1.0 - stability.world_consistency)
        + stability.semantic_drift)
        / 4.0;
    (1.0 - risk).clamp(0.0, 1.0)
}

fn warnings_for(stability: &CausalStability) -> Vec<String> {
    match MutationValidationGate::classify(stability) {
        CausalStabilityLevel::Stable => Vec::new(),
        CausalStabilityLevel::Degrading => {
            vec!["構造安定性の低下が予測されます。".to_string()]
        }
        CausalStabilityLevel::Unstable => {
            vec!["構造不安定化が予測されました。".to_string()]
        }
        CausalStabilityLevel::Contradictory => {
            vec!["矛盾する因果関係が予測されました。".to_string()]
        }
    }
}
