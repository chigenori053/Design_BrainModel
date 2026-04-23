use runtime_core::search_domain::{ScoringWeights, WEIGHT_DIM};
use runtime_core::search_runtime::{MAX_BEAM, MAX_EXPLORATION, MIN_BEAM, MIN_EXPLORATION};

use crate::policy_model::{EpisodeFeedback, SearchPolicy, gradient_from_feedback};
use crate::policy_store::PolicyStore;

/// Configuration for the D.2 optimizer — all knobs are deterministic.
#[derive(Clone, Debug, PartialEq)]
pub struct OptimizerConfig {
    /// Learning rate η for weight updates.
    pub learning_rate: f64,
    /// Maximum allowed weight delta per step (Δ ≤ ε).
    pub max_delta: f64,
    /// Weight decay applied each update.
    pub decay_factor: f64,
    /// Sliding window size N for windowed learning.
    pub window_size: usize,
    /// Fraction change in beam exploration step.
    pub beam_step: f64,
    /// Fraction change in exploration rate step.
    pub exploration_step: f64,
    /// Number of consecutive score drops to trigger degradation.
    pub drift_window: usize,
    /// Minimum rolling-average score improvement to count as stable.
    pub drift_threshold: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            max_delta: 0.05,
            decay_factor: 0.999,
            window_size: 64,
            beam_step: 1.0,
            exploration_step: 0.05,
            drift_window: 8,
            drift_threshold: 0.0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 8.1 optimizer sub-components
// ──────────────────────────────────────────────────────────────────────────────

/// Estimates gradient deterministically from a fixed history window.
pub struct GradientEstimator;

impl GradientEstimator {
    pub fn estimate(window: &[EpisodeFeedback]) -> [f64; WEIGHT_DIM] {
        gradient_from_feedback(window)
    }
}

/// Adjusts beam width based on observed success rate trend.
pub struct BeamOptimizer;

impl BeamOptimizer {
    /// Returns the updated beam width.
    /// If success rate improved: narrow the beam (efficiency).
    /// Otherwise: widen the beam (more exploration).
    pub fn update(current: usize, success_rate_improved: bool) -> usize {
        if success_rate_improved {
            current.saturating_sub(1).max(MIN_BEAM)
        } else {
            (current + 1).min(MAX_BEAM)
        }
    }
}

/// Adjusts exploration rate based on stuck detection.
pub struct ExplorationOptimizer;

impl ExplorationOptimizer {
    pub fn update(current: f64, stuck: bool, step: f64) -> f64 {
        if stuck {
            (current + step).clamp(MIN_EXPLORATION, MAX_EXPLORATION)
        } else {
            (current - step).clamp(MIN_EXPLORATION, MAX_EXPLORATION)
        }
    }
}

/// Detects score drift and manages revert-to-stable logic.
pub struct StabilityManager {
    score_window: std::collections::VecDeque<f64>,
    drift_window: usize,
    drift_threshold: f64,
    pub degradation_detected: bool,
}

impl StabilityManager {
    pub fn new(drift_window: usize, drift_threshold: f64) -> Self {
        Self {
            score_window: std::collections::VecDeque::with_capacity(drift_window + 1),
            drift_window,
            drift_threshold,
            degradation_detected: false,
        }
    }

    pub fn observe(&mut self, score: f64) {
        self.score_window.push_back(score);
        if self.score_window.len() > self.drift_window {
            self.score_window.pop_front();
        }
        if self.score_window.len() == self.drift_window {
            let half = self.drift_window / 2;
            let first_half: f64 =
                self.score_window.iter().take(half).sum::<f64>() / half as f64;
            let second_half: f64 =
                self.score_window.iter().skip(half).sum::<f64>() / half as f64;
            self.degradation_detected = second_half < first_half - self.drift_threshold;
        }
    }

    pub fn reset(&mut self) {
        self.score_window.clear();
        self.degradation_detected = false;
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Main optimizer
// ──────────────────────────────────────────────────────────────────────────────

/// Objective function coefficients for J = α*success - β*cost - γ*latency + δ*score_gain
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveCoeffs {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub delta: f64,
}

impl Default for ObjectiveCoeffs {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            beta: 0.3,
            gamma: 0.2,
            delta: 0.5,
        }
    }
}

pub fn compute_reward(
    success: bool,
    cost: f64,
    latency: f64,
    score_gain: f64,
    coeffs: &ObjectiveCoeffs,
) -> f64 {
    let success_val = if success { 1.0 } else { -1.0 };
    coeffs.alpha * success_val - coeffs.beta * cost - coeffs.gamma * latency
        + coeffs.delta * score_gain
}

/// D.2 Optimizer: combines all sub-components.
pub struct Optimizer {
    config: OptimizerConfig,
    stability: StabilityManager,
    history: Vec<EpisodeFeedback>,
    next_version: u64,
}

impl Optimizer {
    pub fn new(config: OptimizerConfig) -> Self {
        let stability = StabilityManager::new(config.drift_window, config.drift_threshold);
        Self {
            config,
            stability,
            history: Vec::new(),
            next_version: 1,
        }
    }

    /// Record a new feedback episode and run one offline optimization step.
    /// Returns the updated policy and whether it should be saved as stable,
    /// or the last-stable policy if degradation was detected.
    pub fn step(
        &mut self,
        current_policy: &SearchPolicy,
        feedback: EpisodeFeedback,
        store: &PolicyStore,
    ) -> (SearchPolicy, bool) {
        self.stability.observe(feedback.score);
        self.history.push(feedback);

        // Revert if degradation detected.
        if self.stability.degradation_detected {
            if let Some(stable) = store.last_stable() {
                self.stability.reset();
                return (stable.clone(), false);
            }
        }

        // Windowed history slice.
        let start = self.history.len().saturating_sub(self.config.window_size);
        let window = &self.history[start..];

        // Determine success rate trend from window.
        let (success_count, prev_success_count) = self.window_success_trend(window);
        let success_rate_improved = success_count > prev_success_count;

        // Detect stuck: no success in last half of the window.
        let half = window.len() / 2;
        let stuck = half > 0 && window.iter().skip(half).all(|f| !f.success);

        // 6.2 Deterministic gradient.
        let raw_grad = GradientEstimator::estimate(window);

        // 6.1 Weight update: clamp delta by max_delta.
        let mut clamped_delta = [0.0f64; WEIGHT_DIM];
        for (i, g) in raw_grad.iter().enumerate() {
            let d = self.config.learning_rate * g;
            clamped_delta[i] = d.clamp(-self.config.max_delta, self.config.max_delta);
        }

        // Apply decay then update.
        let mut new_weights_arr = current_policy.weights.0;
        for w in &mut new_weights_arr {
            *w *= self.config.decay_factor;
        }
        let decayed = ScoringWeights(new_weights_arr);
        let new_weights = decayed.update(&clamped_delta);

        // 6.3 Beam update.
        let new_beam = BeamOptimizer::update(current_policy.beam_width, success_rate_improved);

        // 6.4 Exploration update.
        let new_exploration = ExplorationOptimizer::update(
            current_policy.exploration_rate,
            stuck,
            self.config.exploration_step,
        );

        let version = self.next_version;
        self.next_version += 1;

        let new_policy = SearchPolicy {
            weights: new_weights,
            beam_width: new_beam,
            exploration_rate: new_exploration,
            version,
        }
        .clamped();

        // Mark stable if no degradation and we saw improvement.
        let is_stable = !self.stability.degradation_detected && success_rate_improved;
        (new_policy, is_stable)
    }

    fn window_success_trend(&self, window: &[EpisodeFeedback]) -> (usize, usize) {
        let half = window.len() / 2;
        if half == 0 {
            return (0, 0);
        }
        let prev = window[..half].iter().filter(|f| f.success).count();
        let curr = window[half..].iter().filter(|f| f.success).count();
        (curr, prev)
    }
}
