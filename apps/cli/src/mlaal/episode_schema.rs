use serde::{Deserialize, Serialize};

use crate::nl::types::PlannedStep;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeRecord {
    pub request_fingerprint: String,
    pub dependency_signature: String,
    pub rollout_path: Vec<PlannedStep>,
    pub final_score: f32,
    pub rollback_free: bool,
    pub protected_safe: bool,
    pub replay_trace_hash: String,
    #[serde(default)]
    pub created_at_secs: u64,
    #[serde(default)]
    pub rollback_free_history: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallResult {
    pub matched_episode: Option<EpisodeRecord>,
    pub resonance_score: f32,
    pub recommended_depth: usize,
    pub can_skip_rollout: bool,
}

impl Default for RecallResult {
    fn default() -> Self {
        Self {
            matched_episode: None,
            resonance_score: 0.0,
            recommended_depth: 3,
            can_skip_rollout: false,
        }
    }
}
