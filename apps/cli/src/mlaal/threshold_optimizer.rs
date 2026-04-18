use super::adaptive_policy::AdaptivePolicy;
use super::telemetry_schema::TelemetryWindowKpi;

#[derive(Debug, Default)]
pub struct ThresholdOptimizer;

impl ThresholdOptimizer {
    pub fn optimize(&self, current: &AdaptivePolicy, kpi: &TelemetryWindowKpi) -> AdaptivePolicy {
        let mut next = current.clone();

        if kpi.recall_hit_rate < 0.15 {
            next.resonance_threshold = (next.resonance_threshold - 0.01).clamp(0.80, 0.99);
        }
        if kpi.safe_reuse_success_rate > 0.0 && kpi.safe_reuse_success_rate < 0.95 {
            next.resonance_threshold = (next.resonance_threshold + 0.02).clamp(0.80, 0.99);
            next.skip_threshold = (next.skip_threshold + 0.02).clamp(0.85, 0.995);
        } else if kpi.rollout_skip_rate > 0.0 && kpi.safe_reuse_success_rate > 0.98 {
            next.skip_threshold = (next.skip_threshold - 0.01).clamp(0.85, 0.995);
        }

        if kpi.replay_drift_rate > 0.20 {
            next.decay_lambda = (next.decay_lambda + 0.01).clamp(0.02, 0.25);
            next.depth_shrink_ratio = (next.depth_shrink_ratio + 0.10).clamp(0.10, 0.90);
        }

        if kpi.safe_reuse_success_rate > 0.98 {
            next.beam_shrink_ratio = (next.beam_shrink_ratio + 0.10).clamp(0.20, 0.90);
            next.depth_shrink_ratio = (next.depth_shrink_ratio + 0.10).clamp(0.10, 0.90);
        }

        if kpi.avg_preview_latency_ms > 1_000.0 {
            next.skip_threshold = (next.skip_threshold - 0.01).clamp(0.85, 0.995);
            next.beam_shrink_ratio = (next.beam_shrink_ratio + 0.05).clamp(0.20, 0.90);
        }

        next
    }
}
