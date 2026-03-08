#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_depth: usize,
    pub pruning_threshold: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 5,
            max_depth: 4,
            pruning_threshold: 0.25,
        }
    }
}
