use std::path::PathBuf;

use crate::ir::{LoadedCheckpoint, ReplayTimelineEntry};
use crate::nl::types::PlannedStep;

pub type IrCheckpoint = LoadedCheckpoint;
pub type ReplayTimeline = Vec<ReplayTimelineEntry>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DependencyGraph {
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RollbackState {
    pub rollback_available: bool,
    pub active_transaction_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CognitiveContext {
    pub target: PathBuf,
    pub user_request: String,
    pub ir_checkpoint: Option<IrCheckpoint>,
    pub replay_timeline: Option<ReplayTimeline>,
    pub dependency_graph: Option<DependencyGraph>,
    pub rollback_state: Option<RollbackState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningConstraints {
    pub preview_required: bool,
    pub rollback_safe: bool,
    pub protected_branch: bool,
    pub max_rollout_depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanResult {
    pub selected_action: PlannedStep,
    pub confidence: f32,
    pub risk_score: f32,
    pub compatibility_mode: bool,
    pub planned_steps: Vec<PlannedStep>,
}

pub trait ReasoningPlanner: Send + Sync {
    fn plan(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
    ) -> anyhow::Result<PlanResult>;
}
