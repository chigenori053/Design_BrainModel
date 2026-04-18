use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::nl::types::PlannedStep;

use super::planner::{CognitiveContext, ReplayTimeline};
use super::rollout::PatchCandidate;

#[derive(Debug, Default)]
pub struct MemoryBridge;

impl MemoryBridge {
    pub fn workspace_root(&self, ctx: &CognitiveContext) -> PathBuf {
        if let Some(checkpoint) = ctx.ir_checkpoint.as_ref() {
            return checkpoint.state.workspace_root.clone();
        }
        infer_workspace_root(&ctx.target)
    }

    pub fn request_fingerprint(&self, ctx: &CognitiveContext) -> String {
        stable_hash(&normalize_request(&ctx.user_request))
    }

    pub fn dependency_signature(
        &self,
        ctx: &CognitiveContext,
        candidate: &PatchCandidate,
    ) -> String {
        let mut parts = Vec::new();
        parts.push(
            ctx.dependency_graph
                .as_ref()
                .map(|graph| graph.nodes.join("|"))
                .unwrap_or_else(|| {
                    candidate
                        .estimated_files
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join("|")
                }),
        );
        parts.push(candidate.diff_preview.summary.to_lowercase());
        stable_hash(&parts.join("::"))
    }

    pub fn replay_trace_hash(&self, timeline: Option<&ReplayTimeline>) -> String {
        let trace = timeline
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| format!("{}:{}:{}", entry.step, entry.action, entry.state_hash))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .unwrap_or_else(|| "no-trace".to_string());
        stable_hash(&trace)
    }

    pub fn rollout_path_pattern(&self, steps: &[PlannedStep]) -> String {
        stable_hash(
            &steps
                .iter()
                .map(step_pattern)
                .collect::<Vec<_>>()
                .join("->"),
        )
    }

    pub fn rollback_lineage_changed(
        &self,
        ctx: &CognitiveContext,
        episode_trace_hash: &str,
    ) -> bool {
        self.replay_trace_hash(ctx.replay_timeline.as_ref()) != episode_trace_hash
    }

    pub fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}

fn infer_workspace_root(target: &Path) -> PathBuf {
    if target.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    if target.is_dir() {
        return target.to_path_buf();
    }
    target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn normalize_request(request: &str) -> String {
    request
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn step_pattern(step: &PlannedStep) -> String {
    match step {
        PlannedStep::Analyze(_) => "analyze".to_string(),
        PlannedStep::Coding(_, _) => "coding".to_string(),
        PlannedStep::Validate(_) => "validate".to_string(),
        PlannedStep::ApplyPreviousCodingStep => "apply_previous".to_string(),
        PlannedStep::RollbackCurrentTransaction => "rollback".to_string(),
        PlannedStep::StructureView(_) => "structure_view".to_string(),
        PlannedStep::StructureEdit(_) => "structure_edit".to_string(),
        PlannedStep::StructureDiff(_, _) => "structure_diff".to_string(),
        PlannedStep::StructureUndo(_) => "structure_undo".to_string(),
        PlannedStep::StructureRedo(_) => "structure_redo".to_string(),
        PlannedStep::Run(_) => "run".to_string(),
        PlannedStep::Rules => "rules".to_string(),
        PlannedStep::Memory(_) => "memory".to_string(),
        PlannedStep::GitCommit(_) => "git_commit".to_string(),
        PlannedStep::GitPR(_) => "git_pr".to_string(),
        PlannedStep::AlternativeMutationSearch(_) => "alt_search".to_string(),
        PlannedStep::DesignDeltaReasoning(_) => "design_delta".to_string(),
        PlannedStep::ExplainDesignTradeoff(_) => "tradeoff".to_string(),
    }
}

fn stable_hash(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
