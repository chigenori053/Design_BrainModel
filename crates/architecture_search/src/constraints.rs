use architecture_ir::{ArchitectureIR, validate_ir};

use crate::{
    ArchitectureGrammar, ArchitectureGrammarEngine, SearchSpace, SearchState,
};

pub trait ConstraintFilter {
    fn filter(&self, candidates: Vec<SearchState>) -> Vec<SearchState>;
}

#[derive(Clone, Debug)]
pub struct BasicConstraintFilter {
    search_space: SearchSpace,
}

impl BasicConstraintFilter {
    pub fn new(search_space: SearchSpace) -> Self {
        Self { search_space }
    }
}

impl ConstraintFilter for BasicConstraintFilter {
    fn filter(&self, candidates: Vec<SearchState>) -> Vec<SearchState> {
        let mut filtered = candidates
            .into_iter()
            .filter(|state| validate_ir(&state.architecture).is_valid())
            .filter(|state| allowed_by_rules(&state.architecture, &self.search_space))
            .collect::<Vec<_>>();

        filtered.sort_by(|lhs, rhs| {
            lhs.depth
                .cmp(&rhs.depth)
                .then_with(|| lhs.state_id.cmp(&rhs.state_id))
        });
        filtered
    }
}

fn allowed_by_rules(ir: &ArchitectureIR, search_space: &SearchSpace) -> bool {
    if ir
        .components
        .iter()
        .any(|component| search_space.forbidden_components.contains(&component.component_type))
    {
        return false;
    }

    let grammar = ArchitectureGrammar {
        style: crate::ArchitectureStyle::Generic,
        component_rules: search_space.component_rules.clone(),
        dependency_rules: search_space.allowed_dependencies.clone(),
        layer_rules: search_space.layer_rules.clone(),
        interface_rules: search_space.interface_rules.clone(),
        constraint_rule: search_space.constraint_rule.clone(),
    };
    ArchitectureGrammarEngine.validate(ir, &grammar).valid
}
