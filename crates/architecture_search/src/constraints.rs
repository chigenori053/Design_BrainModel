use architecture_ir::{ArchitectureIR, ComponentType, NodeId, validate_ir};

use crate::{SearchSpace, SearchState};

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
    ir.dependencies
        .iter()
        .all(|edge| match (edge.source, edge.target) {
            (NodeId::Component(from), NodeId::Component(to)) => {
                match (component_type(ir, from), component_type(ir, to)) {
                    (Some(from_type), Some(to_type)) => search_space
                        .allowed_dependencies
                        .iter()
                        .any(|rule| rule.from == *from_type && rule.to == *to_type),
                    _ => false,
                }
            }
            _ => true,
        })
}

fn component_type(ir: &ArchitectureIR, component_id: u64) -> Option<&ComponentType> {
    ir.components
        .iter()
        .find(|component| component.id == component_id)
        .map(|component| &component.component_type)
}
