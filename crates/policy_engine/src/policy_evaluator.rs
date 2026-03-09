use std::collections::HashMap;

use memory_space_phase14::DesignPattern;
use world_model_core::WorldState;

use crate::pattern_generalizer::generalize_architecture;
use crate::policy_model::{ActionType, SearchPolicy};

pub type ActionWeights = HashMap<ActionType, f64>;

pub fn evaluate_policy(
    state: &WorldState,
    patterns: &[DesignPattern],
    policy: Option<&SearchPolicy>,
) -> ActionWeights {
    let abstract_state = generalize_architecture(&state.architecture);
    let mut weights = HashMap::new();

    if let Some(policy) = policy {
        for (action_type, weight) in &policy.action_weights {
            weights.insert(*action_type, *weight);
        }
        for pattern in patterns {
            if let Some(pattern_weight) = policy.pattern_weights.get(&pattern.pattern_id) {
                let role_match = if pattern.layer_sequence.len() == abstract_state.node_roles.len()
                {
                    1.0
                } else {
                    1.0 / (1.0
                        + pattern
                            .layer_sequence
                            .len()
                            .abs_diff(abstract_state.node_roles.len())
                            as f64)
                };
                for action_type in preferred_actions_for_pattern(pattern) {
                    weights
                        .entry(action_type)
                        .and_modify(|value| *value = quantize(*value + pattern_weight * role_match))
                        .or_insert(quantize(pattern_weight * role_match));
                }
            }
        }
    }

    weights
}

fn preferred_actions_for_pattern(pattern: &DesignPattern) -> Vec<ActionType> {
    let mut out = pattern
        .layer_sequence
        .iter()
        .map(|layer| match layer {
            design_domain::Layer::Ui => ActionType::AddUi,
            design_domain::Layer::Service => ActionType::AddService,
            design_domain::Layer::Repository => ActionType::AddRepository,
            design_domain::Layer::Database => ActionType::AddDatabase,
        })
        .collect::<Vec<_>>();
    if !pattern.dependency_edges.is_empty() {
        out.push(ActionType::ConnectDependency);
    }
    out.sort();
    out.dedup();
    out
}

fn quantize(value: f64) -> f64 {
    ((value * 100.0).round() / 100.0).clamp(0.0, 1.0)
}
