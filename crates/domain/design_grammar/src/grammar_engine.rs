use design_domain::{Architecture, Constraint, Layer};
use semantic_domain::Intent;
use world_model_core::WorldState;

use crate::{
    architecture_rules::validate_architecture_rules,
    constraint_rules::{validate_constraint_rules, ConstraintRule},
    dependency_rules::validate_dependency_rules,
    validation::GrammarValidation,
};

#[derive(Clone, Debug, Default)]
pub struct GrammarEngine {
    pub constraint_rules: ConstraintRule,
}

impl GrammarEngine {
    pub fn validate_architecture(&self, architecture: &Architecture) -> GrammarValidation {
        let mut messages = validate_architecture_rules(architecture);
        messages.extend(validate_dependency_rules(architecture));
        GrammarValidation::from_messages(messages)
    }

    pub fn validate_dependency(&self, world_state: &WorldState) -> GrammarValidation {
        GrammarValidation::from_messages(validate_dependency_rules(&world_state.architecture))
    }

    pub fn validate_design_unit(&self, world_state: &WorldState) -> GrammarValidation {
        let mut messages = Vec::new();
        for class_unit in &world_state.architecture.classes {
            for structure in &class_unit.structures {
                for unit in &structure.design_units {
                    if unit.outputs.len() > unit.inputs.len() + 2 {
                        messages.push(format!("design unit '{}' has incompatible io shape", unit.name));
                    }
                    if unit.dependencies.contains(&unit.id) {
                        messages.push(format!("design unit '{}' has circular data flow", unit.name));
                    }
                }
            }
        }
        GrammarValidation::from_messages(messages)
    }

    pub fn validate_constraints(&self, world_state: &WorldState) -> GrammarValidation {
        GrammarValidation::from_messages(validate_constraint_rules(
            &world_state.architecture,
            &world_state.constraints,
            &self.constraint_rules,
        ))
    }

    pub fn validate_world_state(&self, world_state: &WorldState) -> GrammarValidation {
        let mut messages = self
            .validate_architecture(&world_state.architecture)
            .issues
            .into_iter()
            .map(|issue| issue.message)
            .collect::<Vec<_>>();
        messages.extend(
            self.validate_design_unit(world_state)
                .issues
                .into_iter()
                .map(|issue| issue.message),
        );
        messages.extend(
            self.validate_constraints(world_state)
                .issues
                .into_iter()
                .map(|issue| issue.message),
        );
        GrammarValidation::from_messages(messages)
    }

    pub fn constraints_from_intent(&self, intent: &Intent) -> Vec<Constraint> {
        let intent_name = intent.name.to_ascii_lowercase();
        if intent_name.contains("web api") || intent_name.contains("api") {
            return vec![
                Constraint {
                    name: "controller_layer".to_string(),
                    max_design_units: Some(32),
                    max_dependencies: Some(64),
                },
                Constraint {
                    name: "service_layer".to_string(),
                    max_design_units: Some(32),
                    max_dependencies: Some(64),
                },
            ];
        }

        vec![Constraint {
            name: format!("intent:{}", intent.name),
            max_design_units: Some(50),
            max_dependencies: Some(200),
        }]
    }

    pub fn preferred_layers_for_intent(&self, intent: &Intent) -> Vec<Layer> {
        let intent_name = intent.name.to_ascii_lowercase();
        if intent_name.contains("web api") || intent_name.contains("api") {
            return vec![Layer::Ui, Layer::Service, Layer::Repository];
        }
        vec![Layer::Service]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_domain::{Dependency, DependencyKind, DesignUnit, DesignUnitId};

    #[test]
    fn grammar_engine_rejects_reverse_layer_dependency() {
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::with_layer(1, "ControllerUnit", Layer::Ui));
        architecture.add_design_unit(DesignUnit::with_layer(2, "DatabaseUnit", Layer::Database));
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(2),
            to: DesignUnitId(1),
            kind: DependencyKind::Calls,
        });

        let validation = GrammarEngine::default().validate_architecture(&architecture);

        assert!(!validation.valid);
    }
}
