use runtime_core::search_domain::{FeatureVector, ScoringWeights, WEIGHT_DIM};
use runtime_core::search_runtime::{MAX_BEAM, MAX_EXPLORATION, MIN_BEAM, MIN_EXPLORATION};

/// Feedback entry from a single search episode.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeFeedback {
    pub success: bool,
    pub features: FeatureVector,
    /// Reward signal J = α*success - β*cost - γ*latency + δ*score_gain
    pub reward: f64,
    /// Raw score obtained during this episode.
    pub score: f64,
}

/// D.2 SearchPolicy — wraps scoring weights and runtime search parameters.
/// Deterministic: same feedback history produces identical policy.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchPolicy {
    pub weights: ScoringWeights,
    pub beam_width: usize,
    pub exploration_rate: f64,
    pub version: u64,
}

impl SearchPolicy {
    pub fn initial() -> Self {
        Self {
            weights: ScoringWeights::uniform(),
            beam_width: 8,
            exploration_rate: 0.1,
            version: 0,
        }
    }

    /// Monotonic clamp: ensure all values stay within their specified bounds.
    pub fn clamped(self) -> Self {
        Self {
            weights: self.weights.clamped(),
            beam_width: self.beam_width.clamp(MIN_BEAM, MAX_BEAM),
            exploration_rate: self.exploration_rate.clamp(MIN_EXPLORATION, MAX_EXPLORATION),
            version: self.version,
        }
    }

    pub fn with_version(mut self, v: u64) -> Self {
        self.version = v;
        self
    }
}

impl Default for SearchPolicy {
    fn default() -> Self {
        Self::initial()
    }
}

/// Deterministic gradient estimate from a fixed window of feedback.
/// gradient_estimate = Σ_i (feedback_i.reward * feedback_i.features[dim])
pub fn gradient_from_feedback(window: &[EpisodeFeedback]) -> [f64; WEIGHT_DIM] {
    let mut grad = [0.0f64; WEIGHT_DIM];
    for fb in window {
        for (i, g) in grad.iter_mut().enumerate() {
            *g += fb.reward * fb.features.0[i];
        }
    }
    grad
}
