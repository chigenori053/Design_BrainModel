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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationDecision {
    Allow,
    Warn,
    Reject,
}

pub struct MutationValidationGate;

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

    pub fn narrative(decision: ValidationDecision) -> &'static str {
        match decision {
            ValidationDecision::Allow => {
                "変更内容を評価しました。\n\n構造安定性は維持されています。\n\n変更を適用できます。"
            }
            ValidationDecision::Warn => {
                "変更内容を評価しました。\n\n構造安定性の低下が予測されます。\n\n適用は可能ですが、影響範囲の確認を推奨します。"
            }
            ValidationDecision::Reject => {
                "変更内容を評価しました。\n\n構造不安定化が予測されたため、変更は拒否されました。\n\n推奨:\n・責務境界を見直す\n・依存関係を整理する"
            }
        }
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
