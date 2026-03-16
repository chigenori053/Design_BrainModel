#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_depth: usize,
    pub max_candidates: usize,
    pub pareto_limit: usize,
    pub timeout_ms: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 8,
            max_depth: 6,
            max_candidates: 1_024,
            pareto_limit: 10,
            timeout_ms: 10_000,
        }
    }
}
