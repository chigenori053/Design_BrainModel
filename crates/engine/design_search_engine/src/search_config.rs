#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    pub max_depth: usize,
    pub max_candidates: usize,
    pub beam_width: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_candidates: 64,
            beam_width: 8,
        }
    }
}
