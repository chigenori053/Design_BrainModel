use std::collections::HashMap;

use design_domain::Layer;
use world_model_core::{Action, WorldState};

use crate::pattern_store::DesignPattern;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchPrior {
    pub action_weights: HashMap<Action, f64>,
}

impl SearchPrior {
    pub fn from_patterns(
        state: &WorldState,
        patterns: &[DesignPattern],
        actions: &[Action],
    ) -> Self {
        let current_layers = state
            .architecture
            .design_units_by_id()
            .values()
            .map(|unit| unit.layer)
            .collect::<Vec<_>>();
        let current_edge_count = state.architecture.causal_graph().edges().len();

        let action_weights = actions
            .iter()
            .cloned()
            .map(|action| {
                let mut weight = 1.0;
                for pattern in patterns {
                    let strength = pattern.average_score * (1.0 + pattern.frequency as f64 * 0.1);
                    match &action {
                        Action::AddDesignUnit { layer, .. } => {
                            let target_count = pattern
                                .layer_sequence
                                .iter()
                                .filter(|candidate| *candidate == layer)
                                .count();
                            let current_count = current_layers
                                .iter()
                                .filter(|candidate| *candidate == layer)
                                .count();
                            if target_count > current_count {
                                weight += strength * 0.25;
                            }
                            if preferred_next_layer(&current_layers, &pattern.layer_sequence)
                                == Some(*layer)
                            {
                                weight += strength * 0.2;
                            }
                        }
                        Action::ConnectDependency { .. } => {
                            if pattern.causal_graph.edges().len() > current_edge_count {
                                weight += strength * 0.2;
                            }
                        }
                        Action::MergeStructure | Action::RemoveDesignUnit => {
                            if state.architecture.design_unit_count()
                                > pattern.causal_graph.nodes().count()
                            {
                                weight += strength * 0.1;
                            }
                        }
                        Action::SplitStructure => {
                            if state.architecture.structure_count()
                                < pattern.layer_sequence.len().max(1)
                            {
                                weight += strength * 0.05;
                            }
                        }
                    }
                }
                (action, weight)
            })
            .collect();

        Self { action_weights }
    }

    pub fn weight_for(&self, action: &Action) -> f64 {
        self.action_weights.get(action).copied().unwrap_or(1.0)
    }
}

fn preferred_next_layer(current_layers: &[Layer], pattern_layers: &[Layer]) -> Option<Layer> {
    for layer in pattern_layers {
        let current = current_layers
            .iter()
            .filter(|candidate| *candidate == layer)
            .count();
        let target = pattern_layers
            .iter()
            .filter(|candidate| *candidate == layer)
            .count();
        if current < target {
            return Some(*layer);
        }
    }
    None
}
