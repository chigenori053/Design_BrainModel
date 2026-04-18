use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryRecord {
    pub recall_hit: bool,
    pub rollout_skipped: bool,
    pub rollout_depth: usize,
    pub beam_width: usize,
    pub preview_latency_ms: u64,
    pub rollback_free: bool,
    pub protected_safe: bool,
    pub resonance_score: f32,
    pub decay_applied: bool,
    #[serde(default)]
    pub replay_divergence: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TelemetryWindowKpi {
    pub avg_rollout_depth: f32,
    pub recall_hit_rate: f32,
    pub rollout_skip_rate: f32,
    pub safe_reuse_success_rate: f32,
    pub avg_preview_latency_ms: f32,
    pub replay_drift_rate: f32,
}
