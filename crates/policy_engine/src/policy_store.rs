use memory_space_phase14::DesignExperience;
use world_model_core::Action;

use crate::pattern_generalizer::generalize_architecture;
use crate::policy_model::{ActionType, SearchPolicy};
use crate::search_policy::action_type_for_action;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolicyStore {
    pub policies: Vec<SearchPolicy>,
}

impl PolicyStore {
    pub fn update_policy(&mut self, experiences: &[DesignExperience]) -> SearchPolicy {
        let mut policy = SearchPolicy::default();
        let mut total_weight = 0.0;

        for experience in experiences {
            let strength = quantize(experience.score);
            total_weight += strength;
            let abstract_pattern = generalize_architecture(&experience.architecture);
            policy
                .pattern_weights
                .entry(abstract_pattern.pattern_id)
                .and_modify(|value| *value = quantize(*value + strength))
                .or_insert(strength);

            for action_type in infer_success_actions(experience) {
                policy
                    .action_weights
                    .entry(action_type)
                    .and_modify(|value| *value = quantize(*value + strength))
                    .or_insert(strength);
            }
        }

        if total_weight > 0.0 {
            for value in policy.action_weights.values_mut() {
                *value = quantize(*value / total_weight).max(0.05);
            }
            for value in policy.pattern_weights.values_mut() {
                *value = quantize(*value / total_weight).max(0.05);
            }
        }

        self.policies.push(policy.clone());
        policy
    }

    pub fn latest(&self) -> Option<&SearchPolicy> {
        self.policies.last()
    }
}

fn infer_success_actions(experience: &DesignExperience) -> Vec<ActionType> {
    let mut actions = Vec::new();
    let architecture = &experience.architecture;
    let mut layers = architecture
        .design_units_by_id()
        .values()
        .map(|unit| unit.layer)
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| layer.order());
    for layer in layers {
        let action = match layer {
            design_domain::Layer::Ui => Action::AddDesignUnit {
                name: "policy-ui".to_string(),
                layer,
            },
            design_domain::Layer::Service => Action::AddDesignUnit {
                name: "policy-service".to_string(),
                layer,
            },
            design_domain::Layer::Repository => Action::AddDesignUnit {
                name: "policy-repo".to_string(),
                layer,
            },
            design_domain::Layer::Database => Action::AddDesignUnit {
                name: "policy-db".to_string(),
                layer,
            },
        };
        actions.push(action_type_for_action(&action));
    }
    if !architecture.dependencies.is_empty() || !experience.dependency_edges.is_empty() {
        actions.push(ActionType::ConnectDependency);
    }
    if architecture.structure_count() > 1 {
        actions.push(ActionType::SplitStructure);
    } else {
        actions.push(ActionType::MergeStructure);
    }
    actions.sort();
    actions.dedup();
    actions
}

fn quantize(value: f64) -> f64 {
    ((value * 100.0).round() / 100.0).clamp(0.0, 1.0)
}
