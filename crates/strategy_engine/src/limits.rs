/// Upper bounds for finite exploration.
///
/// Spec DBM-LIMITS-INTEGRATION-STEP3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_candidates: usize,
    pub max_depth: usize,
    pub max_history: usize,
    pub max_graph_nodes: usize,
    pub max_replay_steps: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_candidates: 3,
            max_depth: 2,
            max_history: 50,
            max_graph_nodes: 1000,
            max_replay_steps: 20,
        }
    }
}
