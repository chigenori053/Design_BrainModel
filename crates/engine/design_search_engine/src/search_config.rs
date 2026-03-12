use crate::audit::CapabilityLimits;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchConfig {
    pub max_depth: usize,
    pub max_candidates: usize,
    pub beam_width: usize,
    pub diversity_threshold: f64,
    pub experience_bias: f64,
    pub policy_bias: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_candidates: 64,
            beam_width: 16,
            diversity_threshold: 0.85,
            experience_bias: 0.2,
            policy_bias: 0.15,
        }
    }
}

impl SearchConfig {
    pub fn apply_capability_limits(&self, limits: &CapabilityLimits) -> Self {
        Self {
            max_depth: self.max_depth.min(limits.max_search_depth as usize),
            beam_width: self.beam_width.min(limits.beam_width as usize).max(1),
            ..*self
        }
    }
}
