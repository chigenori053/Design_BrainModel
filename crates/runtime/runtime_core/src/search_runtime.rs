pub const MIN_BEAM: usize = 1;
pub const MAX_BEAM: usize = 64;
pub const MIN_EXPLORATION: f64 = 0.0;
pub const MAX_EXPLORATION: f64 = 1.0;

/// Runtime search parameters adjusted by the D.2 learning loop.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchRuntimeParams {
    pub beam_width: usize,
    pub exploration_rate: f64,
}

impl SearchRuntimeParams {
    pub fn default_params() -> Self {
        Self {
            beam_width: 8,
            exploration_rate: 0.1,
        }
    }

    /// Adjust beam width: narrow when succeeding (efficiency), widen when failing (exploration).
    pub fn update_beam(&self, success_rate_improved: bool) -> Self {
        let new_beam = if success_rate_improved {
            (self.beam_width.saturating_sub(1)).max(MIN_BEAM)
        } else {
            (self.beam_width + 1).min(MAX_BEAM)
        };
        Self {
            beam_width: new_beam,
            exploration_rate: self.exploration_rate,
        }
    }

    /// Adjust exploration rate: increase when stuck, decrease otherwise.
    pub fn update_exploration(&self, stuck: bool, epsilon: f64) -> Self {
        let delta = epsilon.abs();
        let new_rate = if stuck {
            (self.exploration_rate + delta).clamp(MIN_EXPLORATION, MAX_EXPLORATION)
        } else {
            (self.exploration_rate - delta).clamp(MIN_EXPLORATION, MAX_EXPLORATION)
        };
        Self {
            beam_width: self.beam_width,
            exploration_rate: new_rate,
        }
    }
}

impl Default for SearchRuntimeParams {
    fn default() -> Self {
        Self::default_params()
    }
}
