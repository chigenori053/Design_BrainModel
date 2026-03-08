use concept_engine::ConceptId;

use crate::design_state::DesignState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentNode {
    pub concept: ConceptId,
    pub weight: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ConstraintEngine {
    pub intent_nodes: Vec<IntentNode>,
}

impl ConstraintEngine {
    pub fn is_valid(&self, state: &DesignState) -> bool {
        let hard_limit = (self.intent_nodes.len() * 8).max(8);
        state.design_units.len() <= hard_limit
    }
}
