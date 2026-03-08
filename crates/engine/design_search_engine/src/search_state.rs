use world_model_core::WorldState;

/// Phase9-D search state: wraps a WorldState with search metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    pub state_id: u64,
    pub world_state: WorldState,
    pub depth: usize,
    pub score: f64,
    pub pareto_rank: usize,
}

impl SearchState {
    pub fn new(state_id: u64, world_state: WorldState) -> Self {
        Self {
            state_id,
            world_state,
            depth: 0,
            score: 0.0,
            pareto_rank: 0,
        }
    }
}
