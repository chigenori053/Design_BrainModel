use std::path::PathBuf;

use super::planner::{CognitiveContext, IrCheckpoint, PlanningConstraints};
use super::replay_rollout::ReplayRolloutAdapter;
use crate::nl::types::PlannedStep;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffPreview {
    pub summary: String,
    pub patch_count: usize,
    pub unsafe_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchCandidate {
    pub step: PlannedStep,
    pub diff_preview: DiffPreview,
    pub estimated_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RolloutState {
    pub depth: usize,
    pub predicted_ir: Option<IrCheckpoint>,
    pub divergence_score: f32,
    pub rollback_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRollout {
    pub candidate: PatchCandidate,
    pub state: RolloutState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutEngine {
    pub max_depth: usize,
    pub beam_width: usize,
}

impl Default for RolloutEngine {
    fn default() -> Self {
        Self {
            max_depth: 3,
            beam_width: 4,
        }
    }
}

impl RolloutEngine {
    pub fn evaluate_candidates(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
        replay: &ReplayRolloutAdapter,
        candidates: Vec<PatchCandidate>,
    ) -> Vec<CandidateRollout> {
        let depth = constraints.max_rollout_depth.min(self.max_depth).max(1);
        candidates
            .into_iter()
            .take(self.beam_width)
            .map(|candidate| {
                let predicted_ir =
                    replay.simulate_future_steps(ctx.ir_checkpoint.as_ref(), &candidate, depth);
                let divergence_score =
                    replay.estimate_divergence(ctx.replay_timeline.as_ref(), &candidate, depth);
                let rollback_available = predict_rollback_availability(ctx, &candidate);
                CandidateRollout {
                    candidate,
                    state: RolloutState {
                        depth,
                        predicted_ir,
                        divergence_score,
                        rollback_available,
                    },
                }
            })
            .collect()
    }
}

fn predict_rollback_availability(ctx: &CognitiveContext, candidate: &PatchCandidate) -> bool {
    if matches!(candidate.step, PlannedStep::Reload) {
        return true;
    }
    if matches!(candidate.step, PlannedStep::Apply) {
        return ctx
            .rollback_state
            .as_ref()
            .and_then(|state| state.active_transaction_id.as_ref())
            .is_some();
    }

    let rollback_signal = ctx
        .rollback_state
        .as_ref()
        .map(|state| state.rollback_available || state.active_transaction_id.as_ref().is_some())
        .unwrap_or(true);

    rollback_signal
        && !candidate.diff_preview.unsafe_mutation
        && candidate.estimated_files.len() <= 3
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::nl::types::{PlannedStep, RefactorSpec};

    use super::*;
    use crate::mlaal::{CognitiveContext, ReplayRolloutAdapter, RollbackState};

    #[test]
    fn rollout_prefers_lower_rollback_risk() {
        let engine = RolloutEngine::default();
        let replay = ReplayRolloutAdapter;
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: "安全に trait 抽出して".to_string(),
            rollback_state: Some(RollbackState {
                rollback_available: true,
                active_transaction_id: Some("tx-1".to_string()),
            }),
            ..CognitiveContext::default()
        };
        let constraints = PlanningConstraints {
            preview_required: true,
            rollback_safe: true,
            protected_branch: false,
            max_rollout_depth: 3,
        };
        let safe = PatchCandidate {
            step: PlannedStep::Refactor(RefactorSpec {
                target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                request: "safe refactor".to_string(),
            }),
            diff_preview: DiffPreview {
                summary: "single file safe".to_string(),
                patch_count: 1,
                unsafe_mutation: false,
            },
            estimated_files: vec![PathBuf::from("apps/cli/src/nl/planner_v2.rs")],
        };
        let unsafe_candidate = PatchCandidate {
            step: PlannedStep::Refactor(RefactorSpec {
                target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                request: "unsafe refactor".to_string(),
            }),
            diff_preview: DiffPreview {
                summary: "wide unsafe".to_string(),
                patch_count: 3,
                unsafe_mutation: true,
            },
            estimated_files: vec![
                PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                PathBuf::from("apps/cli/src/nl/mod.rs"),
                PathBuf::from("apps/cli/src/nl/executor.rs"),
            ],
        };

        let rollouts =
            engine.evaluate_candidates(&ctx, &constraints, &replay, vec![safe, unsafe_candidate]);

        assert!(rollouts[0].state.rollback_available);
        assert!(!rollouts[1].state.rollback_available);
    }
}
