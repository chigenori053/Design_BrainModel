use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use crate::ir::IRPersistenceStore;
use crate::nl::session::ConversationState;
use crate::nl::types::{CommandPlan, ExecutionPlan};
use crate::session::AgentSession;

use super::legacy_adapter::{LegacyLookaheadAdapter, LookaheadSimulator};
use super::mlaal_planner::MLAALPlanner;
use super::planner::{CognitiveContext, PlanningConstraints, ReasoningPlanner, RollbackState};
use super::replay_hook::attach_replay_context;
use super::rollout::RolloutEngine;

pub struct CompatibilityBridge {
    planner: Arc<dyn ReasoningPlanner>,
}

impl CompatibilityBridge {
    pub fn new(planner: Arc<dyn ReasoningPlanner>) -> Self {
        Self { planner }
    }

    pub fn plan(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
    ) -> anyhow::Result<CommandPlan> {
        let result = self.planner.plan(ctx, constraints)?;
        let steps = if result.planned_steps.is_empty() {
            vec![result.selected_action]
        } else {
            result.planned_steps
        };
        Ok(CommandPlan {
            intent: None,
            steps,
        })
    }
}

pub fn resolve_command_plan_with_compatibility(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> (Option<ExecutionPlan>, &'static str) {
    let ctx = build_context(input, session, conversation);
    let constraints = build_constraints(session);
    let command_plan = resolve_with_planners(
        Arc::new(MLAALPlanner::default_stack()),
        build_legacy_planner(session, conversation),
        &ctx,
        &constraints,
    );

    match command_plan {
        Ok(plan) => (Some(ExecutionPlan::from(plan)), "nl_v2"),
        Err(err) if err.to_string().contains("legacy planner produced no plan") => (
            crate::nl::planner_v2::plan_input(input, session, conversation),
            "nl_rule_based",
        ),
        Err(_) => (None, "nl_v2"),
    }
}

fn build_legacy_planner(
    session: &AgentSession,
    conversation: &ConversationState,
) -> Arc<dyn ReasoningPlanner> {
    Arc::new(LegacyLookaheadAdapter::new(
        Arc::new(LookaheadSimulator),
        session.clone(),
        conversation.clone(),
    ))
}

fn build_bridge(planner: Arc<dyn ReasoningPlanner>) -> CompatibilityBridge {
    CompatibilityBridge::new(planner)
}

fn resolve_with_planners(
    primary: Arc<dyn ReasoningPlanner>,
    fallback: Arc<dyn ReasoningPlanner>,
    ctx: &CognitiveContext,
    constraints: &PlanningConstraints,
) -> anyhow::Result<CommandPlan> {
    match build_bridge(primary).plan(ctx, constraints) {
        Ok(plan) => Ok(plan),
        Err(err) if is_legacy_fallback_error(&err) => build_bridge(fallback).plan(ctx, constraints),
        Err(err) => Err(err),
    }
}

fn build_constraints(session: &AgentSession) -> PlanningConstraints {
    PlanningConstraints {
        preview_required: true,
        rollback_safe: true,
        protected_branch: current_branch(session)
            .as_deref()
            .map(is_protected_branch)
            .unwrap_or(false),
        max_rollout_depth: RolloutEngine::default().max_depth,
    }
}

fn build_context(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> CognitiveContext {
    let target = conversation
        .last_target
        .clone()
        .or_else(|| session.workspace_root.clone())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut ctx = CognitiveContext {
        target: target.clone(),
        user_request: input.to_string(),
        rollback_state: Some(RollbackState {
            rollback_available: conversation
                .active_transaction()
                .map(|tx| tx.rollback_available)
                .unwrap_or(false),
            active_transaction_id: conversation
                .active_transaction()
                .map(|tx| tx.transaction_id.clone()),
        }),
        ..CognitiveContext::default()
    };

    let workspace_root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let ir = IRPersistenceStore::new(workspace_root);
    if !conversation.ir_state.session_id.is_empty() {
        if let Ok(checkpoint) = ir.load_latest(&conversation.ir_state.session_id) {
            ctx.ir_checkpoint = Some(crate::ir::LoadedCheckpoint {
                step_index: checkpoint.recovered_step,
                state: checkpoint.state,
            });
        }
        let timeline = attach_replay_context(&ir, &conversation.ir_state.session_id);
        if !timeline.is_empty() {
            ctx.replay_timeline = Some(timeline);
        }
    }
    ctx
}

fn current_branch(session: &AgentSession) -> Option<String> {
    let workspace_root = session.workspace_root.as_ref()?;
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(workspace_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
        || branch.starts_with("release/")
        || branch.starts_with("production/")
        || branch.starts_with("hotfix/")
}

#[cfg(test)]
fn planner_unavailable() -> Arc<dyn ReasoningPlanner> {
    struct PlannerUnavailable;
    impl ReasoningPlanner for PlannerUnavailable {
        fn plan(
            &self,
            _ctx: &CognitiveContext,
            _constraints: &PlanningConstraints,
        ) -> anyhow::Result<super::planner::PlanResult> {
            anyhow::bail!("planner unavailable: forced test path")
        }
    }

    Arc::new(PlannerUnavailable)
}

fn is_legacy_fallback_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("planner unavailable")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::ir::{IRPersistenceArtifact, persist_ir_transition, restore_or_initialize_ir_state};
    use crate::service::dto::ActionKind;

    use super::*;

    #[test]
    fn compatibility_bridge_preserves_apply_gate() {
        let session = AgentSession::new();
        let mut conversation = ConversationState::default();
        conversation.start_preview_transaction(PathBuf::from("apps/cli/src/coding.rs"));

        let (plan, label) =
            resolve_command_plan_with_compatibility("coding --apply", &session, &conversation);
        let plan = plan.expect("compatibility plan");

        assert_eq!(label, "nl_v2");
        assert_eq!(plan.operation, crate::nl::types::Operation::Apply);
    }

    #[test]
    fn legacy_adapter_fallback_works() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        let ctx = build_context(
            "apps/cli/src/nl/planner_v2.rs を修正して",
            &session,
            &conversation,
        );
        let constraints = build_constraints(&session);
        let plan = resolve_with_planners(
            planner_unavailable(),
            build_legacy_planner(&session, &conversation),
            &ctx,
            &constraints,
        )
        .expect("fallback plan");

        assert!(!plan.steps.is_empty());
    }

    #[test]
    fn replay_hook_attaches_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = IRPersistenceStore::new(temp.path());
        let recovery = store.recover_or_create().expect("recovery");
        let before = recovery.state.clone();
        let mut after = before.clone();
        after.current_target = Some(PathBuf::from("apps/cli/src/nl/executor.rs"));
        persist_ir_transition(
            &before,
            &after,
            ActionKind::Analyze,
            "analyze executor",
            IRPersistenceArtifact::default(),
        )
        .expect("persist");

        let timeline = attach_replay_context(&store, &after.session_id);
        assert!(!timeline.is_empty());

        let mut session = AgentSession::new();
        session.workspace_root = Some(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = restore_or_initialize_ir_state(temp.path())
            .expect("restore")
            .state;

        let ctx = build_context("executor を見て", &session, &conversation);
        assert!(ctx.ir_checkpoint.is_some());
        assert!(ctx.replay_timeline.is_some());
    }

    #[test]
    fn preview_apply_ux_unchanged() {
        let session = AgentSession::new();
        let mut conversation = ConversationState::default();
        conversation.start_preview_transaction(PathBuf::from("apps/cli/src/coding.rs"));

        let (plan, _) =
            resolve_command_plan_with_compatibility("coding --apply", &session, &conversation);
        let plan = plan.expect("apply plan");

        assert_eq!(plan.operation, crate::nl::types::Operation::Apply);
    }

    #[test]
    fn rollback_runtime_unchanged() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();

        let (plan, _) =
            resolve_command_plan_with_compatibility("rollback", &session, &conversation);
        // rollback might not produce a plan; if it does, it must not be Apply
        assert!(
            plan.is_none()
                || plan.as_ref().unwrap().operation != crate::nl::types::Operation::Apply
        );
    }
}
