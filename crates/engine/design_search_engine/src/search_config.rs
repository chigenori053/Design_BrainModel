#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchConfig {
    pub max_depth: usize,
    pub max_candidates: usize,
    pub beam_width: usize,
    pub experience_bias: f64,
    pub policy_bias: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_candidates: 64,
            beam_width: 8,
            experience_bias: 0.2,
            policy_bias: 0.15,
        }
    }
}
