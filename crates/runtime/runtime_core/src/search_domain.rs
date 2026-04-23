pub const WEIGHT_DIM: usize = 8;
pub const WEIGHT_MIN: f64 = 0.0;
pub const WEIGHT_MAX: f64 = 2.0;

/// Feature vector extracted from a search state for scoring.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureVector(pub [f64; WEIGHT_DIM]);

impl FeatureVector {
    pub fn zero() -> Self {
        Self([0.0; WEIGHT_DIM])
    }
}

/// Scoring weights optimized by the D.2 learning loop.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoringWeights(pub [f64; WEIGHT_DIM]);

impl ScoringWeights {
    pub fn uniform() -> Self {
        Self([1.0 / WEIGHT_DIM as f64; WEIGHT_DIM])
    }

    /// Clamp all weights to [WEIGHT_MIN, WEIGHT_MAX].
    pub fn clamped(mut self) -> Self {
        for w in &mut self.0 {
            *w = w.clamp(WEIGHT_MIN, WEIGHT_MAX);
        }
        self
    }

    /// Apply a bounded update: w_{t+1} = clamp(w_t + delta, min, max).
    pub fn update(&self, delta: &[f64; WEIGHT_DIM]) -> Self {
        let mut next = self.0;
        for (w, d) in next.iter_mut().zip(delta.iter()) {
            *w = (*w + d).clamp(WEIGHT_MIN, WEIGHT_MAX);
        }
        Self(next)
    }
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self::uniform()
    }
}

/// score = base + dot(features, weights)
pub fn compute_score(base: f64, features: &FeatureVector, weights: &ScoringWeights) -> f64 {
    let dot: f64 = features.0.iter().zip(weights.0.iter()).map(|(f, w)| f * w).sum();
    base + dot
}
