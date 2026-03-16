use crate::grammar::ArchitectureGrammar;
use crate::{IntentModel, SearchSpace};

#[derive(Clone, Debug)]
pub struct DesignSpaceBuilder {
    grammar: ArchitectureGrammar,
}

impl DesignSpaceBuilder {
    pub fn new(grammar: ArchitectureGrammar) -> Self {
        Self { grammar }
    }

    pub fn build(&self, intent: &IntentModel) -> SearchSpace {
        let mut component_catalog = self.grammar.component_catalog();
        for component in intent.required_component_types() {
            if !component_catalog.contains(&component) {
                component_catalog.push(component);
            }
        }
        component_catalog.retain(|component| !intent.constraints.forbidden_components.contains(component));

        let allowed_dependencies = self
            .grammar
            .dependency_rules()
            .into_iter()
            .filter(|rule| {
                component_catalog.contains(&rule.from) && component_catalog.contains(&rule.to)
            })
            .collect();

        SearchSpace {
            component_catalog,
            allowed_dependencies,
            constraints: intent.architecture_constraints(),
            forbidden_components: intent.constraints.forbidden_components.clone(),
            component_rules: self.grammar.component_rules.clone(),
            layer_rules: self.grammar.layer_rules.clone(),
            interface_rules: self.grammar.interface_rules.clone(),
            constraint_rule: self.grammar.constraint_rule.clone(),
        }
    }
}
