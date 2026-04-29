use std::sync::Arc;

use anyhow::{Context, anyhow};

use crate::nl::planner_v2;
use crate::nl::session::ConversationState;
use crate::nl::types::{CommandPlan, ExecutionPlan};
use crate::session::AgentSession;

use super::planner::{CognitiveContext, PlanResult, PlanningConstraints, ReasoningPlanner};

#[derive(Debug, Default)]
pub struct LookaheadSimulator;

impl LookaheadSimulator {
    pub fn plan(
        &self,
        input: &str,
        session: &AgentSession,
        conversation: &ConversationState,
    ) -> Option<ExecutionPlan> {
        planner_v2::plan_input(input, session, conversation)
    }
}

pub struct LegacyLookaheadAdapter {
    simulator: Arc<LookaheadSimulator>,
    session: AgentSession,
    conversation: ConversationState,
}

impl LegacyLookaheadAdapter {
    pub fn new(
        simulator: Arc<LookaheadSimulator>,
        session: AgentSession,
        conversation: ConversationState,
    ) -> Self {
        Self {
            simulator,
            session,
            conversation,
        }
    }
}

impl ReasoningPlanner for LegacyLookaheadAdapter {
    fn plan(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
    ) -> anyhow::Result<PlanResult> {
        let exec_plan = self
            .simulator
            .plan(&ctx.user_request, &self.session, &self.conversation)
            .with_context(|| format!("legacy planner produced no plan for {}", ctx.user_request))?;
        // ExecutionPlan → CommandPlan for mlaal-internal PlanResult (PlannedStep based)
        let cmd_plan = CommandPlan::from(&exec_plan);
        let selected_action = cmd_plan
            .steps
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("legacy planner returned an empty plan"))?;

        Ok(PlanResult {
            selected_action: selected_action.clone(),
            confidence: 1.0,
            risk_score: if constraints.rollback_safe { 0.0 } else { 0.25 },
            compatibility_mode: true,
            planned_steps: vec![selected_action],
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::nl::planner_v2;
    use crate::nl::session::ConversationState;
    use crate::nl::types::{CommandPlan, PlannedStep};
    use crate::session::AgentSession;

    use super::*;

    fn constraints() -> PlanningConstraints {
        PlanningConstraints {
            preview_required: true,
            rollback_safe: true,
            protected_branch: false,
            max_rollout_depth: 1,
        }
    }

    #[test]
    fn planner_trait_returns_same_step_as_legacy() {
        let session = AgentSession::new();
        let conversation = ConversationState::default();
        let adapter = LegacyLookaheadAdapter::new(
            Arc::new(LookaheadSimulator),
            session.clone(),
            conversation.clone(),
        );
        let ctx = CognitiveContext {
            target: PathBuf::from("."),
            user_request: "apps/cli/src/nl/planner_v2.rs を修正して".to_string(),
            ..CognitiveContext::default()
        };

        let legacy_exec =
            planner_v2::plan_input(&ctx.user_request, &session, &conversation).expect("legacy");
        let legacy_steps = CommandPlan::from(&legacy_exec).steps;
        let result = adapter.plan(&ctx, &constraints()).expect("adapter result");

        assert_eq!(result.selected_action, legacy_steps[0]);
        assert_eq!(result.planned_steps, legacy_steps);
    }

    #[test]
    fn planner_trait_preserves_apply_gate() {
        let session = AgentSession::new();
        let mut conversation = ConversationState::default();
        conversation.start_preview_transaction(PathBuf::from("apps/cli/src/coding.rs"));
        let adapter =
            LegacyLookaheadAdapter::new(Arc::new(LookaheadSimulator), session, conversation);
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/coding.rs"),
            user_request: "coding --apply".to_string(),
            ..CognitiveContext::default()
        };

        let result = adapter.plan(&ctx, &constraints()).expect("adapter result");
        assert_eq!(result.selected_action, PlannedStep::Apply);
        assert!(result.compatibility_mode);
    }
}
