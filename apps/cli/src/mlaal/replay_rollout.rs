use std::path::PathBuf;

use crate::ir::IRPersistenceStore;
use crate::nl::types::PlannedStep;
use crate::service::dto::ActionKind;

use super::planner::{IrCheckpoint, ReplayTimeline};
use super::replay_hook::attach_replay_context;
use super::rollout::PatchCandidate;

#[derive(Debug, Default)]
pub struct ReplayRolloutAdapter;

impl ReplayRolloutAdapter {
    pub fn attach(&self, ir: &IRPersistenceStore, session_id: &str) -> ReplayTimeline {
        attach_replay_context(ir, session_id)
    }

    pub fn simulate_future_steps(
        &self,
        checkpoint: Option<&IrCheckpoint>,
        candidate_patch: &PatchCandidate,
        depth: usize,
    ) -> Option<IrCheckpoint> {
        let checkpoint = checkpoint?.clone();
        let mut state = checkpoint.state.clone();
        state.current_target = predicted_target(candidate_patch);
        state.next_allowed_actions = predicted_next_actions(candidate_patch);
        Some(IrCheckpoint {
            step_index: checkpoint.step_index.saturating_add(depth),
            state,
        })
    }

    pub fn estimate_divergence(
        &self,
        timeline: Option<&ReplayTimeline>,
        candidate_patch: &PatchCandidate,
        depth: usize,
    ) -> f32 {
        let baseline = timeline.map(|entries| entries.len() as f32).unwrap_or(1.0);
        let patch_pressure = candidate_patch.estimated_files.len() as f32 * 0.18
            + candidate_patch.diff_preview.patch_count as f32 * 0.07;
        let timeline_penalty = if let Some(entries) = timeline {
            let last_action = entries
                .last()
                .map(|entry| entry.action.to_lowercase())
                .unwrap_or_default();
            if last_action.contains("rollback") && is_write_step(&candidate_patch.step) {
                0.20
            } else {
                0.0
            }
        } else {
            0.05
        };
        let unsafe_penalty = if candidate_patch.diff_preview.unsafe_mutation {
            0.20
        } else {
            0.0
        };
        let normalized_depth = depth as f32 / (baseline + depth as f32 + 1.0);
        ((normalized_depth * 0.30) + patch_pressure + timeline_penalty + unsafe_penalty)
            .clamp(0.0, 1.0)
    }
}

fn predicted_target(candidate_patch: &PatchCandidate) -> Option<PathBuf> {
    candidate_patch
        .estimated_files
        .first()
        .cloned()
        .or_else(|| match &candidate_patch.step {
            PlannedStep::Analyze(path)
            | PlannedStep::Coding(path, _)
            | PlannedStep::Validate(path)
            | PlannedStep::StructureView(path)
            | PlannedStep::StructureEdit(path)
            | PlannedStep::StructureUndo(path)
            | PlannedStep::StructureRedo(path)
            | PlannedStep::Run(path)
            | PlannedStep::Memory(path)
            | PlannedStep::GitCommit(path)
            | PlannedStep::GitPR(path)
            | PlannedStep::StructureDiff(path, _) => Some(path.clone()),
            _ => None,
        })
}

fn predicted_next_actions(candidate_patch: &PatchCandidate) -> Vec<ActionKind> {
    match candidate_patch.step {
        PlannedStep::Analyze(_) => vec![ActionKind::Refactor, ActionKind::Analyze],
        PlannedStep::Validate(_) => vec![ActionKind::Refactor, ActionKind::Analyze],
        PlannedStep::RollbackCurrentTransaction => {
            vec![
                ActionKind::CodingPreview,
                ActionKind::Analyze,
                ActionKind::Refactor,
            ]
        }
        _ => vec![
            ActionKind::Apply,
            ActionKind::Validate,
            ActionKind::Rollback,
        ],
    }
}

fn is_write_step(step: &PlannedStep) -> bool {
    matches!(
        step,
        PlannedStep::Coding(_, _) | PlannedStep::ApplyPreviousCodingStep
    )
}
