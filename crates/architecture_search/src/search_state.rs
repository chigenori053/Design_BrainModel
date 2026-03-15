use architecture_ir::ArchitectureIR;

use crate::evaluator::ArchitectureScore;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchState {
    pub state_id: u64,
    pub architecture: ArchitectureIR,
    pub score: ArchitectureScore,
    pub depth: usize,
}

pub fn create_initial_state() -> SearchState {
    SearchState {
        state_id: 0,
        architecture: ArchitectureIR::default(),
        score: ArchitectureScore::default(),
        depth: 0,
    }
}
