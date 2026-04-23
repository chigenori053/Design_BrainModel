use memory_space_core::RecallResult;
use world_model_core::WorldState;

pub const MIN_BEAM: usize = 1;
pub const MAX_BEAM: usize = 64;

#[derive(Clone, Debug)]
pub struct SearchInput {
    pub world_state: WorldState,
    pub recall: Option<RecallResult>,
    pub max_depth: usize,
    pub max_candidates: usize,
}

impl SearchInput {
    pub fn new(world_state: WorldState) -> Self {
        Self {
            world_state,
            recall: None,
            max_depth: 4,
            max_candidates: 64,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub states: Vec<ScoredState>,
    pub explored_count: usize,
    pub depth_best_scores: Vec<f64>,
}

#[derive(Clone, Debug)]
pub struct ScoredState {
    pub world_state: WorldState,
    pub score: f64,
    pub base_score: f64,
    pub prior_score: f64,
    pub policy_score: f64,
    pub depth: usize,
}

/// Policy parameters consumed by runtime_core::search.
/// beam_width and exploration_rate drive structural behaviour;
/// weights are applied to (prior_score, policy_score) features.
#[derive(Clone, Debug, Default)]
pub struct SearchPolicy {
    pub beam_width: usize,
    pub exploration_rate: f64,
    pub weights: Vec<f64>,
}

impl SearchPolicy {
    pub fn load() -> Self {
        Self::default()
    }
}

/// score = base_score + dot(features, weights), clamped to [0, 1].
/// Monotonic and stable: adding positive weight to a positive feature
/// never decreases the score.
pub fn apply_score_weights(base_score: f64, features: &[f64], weights: &[f64]) -> f64 {
    let dot: f64 = features
        .iter()
        .zip(weights.iter())
        .map(|(f, w)| f * w)
        .sum();
    (base_score + dot).clamp(0.0, 1.0)
}
