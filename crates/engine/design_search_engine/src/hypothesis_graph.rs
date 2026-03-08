use std::collections::HashMap;

use crate::design_state::{DesignState, DesignStateId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignOperation {
    AddUnit,
    RemoveUnit,
    ModifyDependency,
    RefactorStructure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesignTransition {
    pub from: DesignStateId,
    pub to: DesignStateId,
    pub operation: DesignOperation,
}

#[derive(Clone, Debug, Default)]
pub struct HypothesisGraph {
    pub states: HashMap<DesignStateId, DesignState>,
    pub edges: Vec<DesignTransition>,
}

impl HypothesisGraph {
    pub fn insert_state(&mut self, state: DesignState) {
        self.states.insert(state.id, state);
    }

    pub fn add_transition(&mut self, transition: DesignTransition) {
        self.edges.push(transition);
    }

    pub fn best_state(&self) -> Option<&DesignState> {
        self.states.values().max_by(|lhs, rhs| {
            let ls = lhs.evaluation.as_ref().map(|e| e.total()).unwrap_or(0.0);
            let rs = rhs.evaluation.as_ref().map(|e| e.total()).unwrap_or(0.0);
            ls.total_cmp(&rs).then_with(|| rhs.id.cmp(&lhs.id))
        })
    }
}
