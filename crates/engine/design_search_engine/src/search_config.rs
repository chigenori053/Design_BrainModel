#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_iterations: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 8,
            max_iterations: 20,
        }
    }
}
