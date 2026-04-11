use anyhow::Result;

use super::commit_guard::{CommitGuard, CommitGuardDecision};
use super::promotion::RepairLoopContext;
use super::retry_policy::{RetryDecision, RetryEvaluator};
use super::state::{EscalationReason, FailureClass, LoopEntryState, ReplLoopState, RetryPolicy};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepairLoopEvent {
    PromptAccepted,
    SessionResumed,
    AnalysisSucceeded,
    AnalysisAmbiguous,
    AnalysisUnsafe,
    PatchPlanned,
    PatchUnsafe,
    NoViablePatch,
    SandboxApplied,
    SandboxApplyFailed,
    VerifyPassed,
    VerifyFailed,
    FailureClassified(FailureClass),
    FailureUnknown,
    CommitApproved,
    CommitSkipped,
    CommitFailed,
    RollbackSucceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateTransition {
    pub from: ReplLoopState,
    pub event: RepairLoopEvent,
    pub to: ReplLoopState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairLoopStatus {
    pub state: ReplLoopState,
    pub stop_reason: Option<EscalationReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopOutcome {
    pub status: RepairLoopStatus,
    pub context: RepairLoopContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition {
        state: ReplLoopState,
        event: RepairLoopEvent,
    },
}

pub struct RepairLoopController {
    state: ReplLoopState,
    policy: RetryPolicy,
    previous_failure: Option<FailureClass>,
}

impl RepairLoopController {
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            state: ReplLoopState::Idle,
            policy,
            previous_failure: None,
        }
    }

    pub fn state(&self) -> ReplLoopState {
        self.state
    }

    pub fn start(&mut self, ctx: RepairLoopContext) -> Result<LoopOutcome> {
        self.start_from(ctx, LoopEntryState::Analyze)
    }

    pub fn start_from(
        &mut self,
        ctx: RepairLoopContext,
        entry: LoopEntryState,
    ) -> Result<LoopOutcome> {
        self.state = Self::normalize_entry(entry);
        Ok(LoopOutcome {
            status: self.status(),
            context: ctx,
        })
    }

    fn normalize_entry(entry: LoopEntryState) -> ReplLoopState {
        match entry {
            LoopEntryState::Analyze => ReplLoopState::Analyze,
            LoopEntryState::PlanPatch => ReplLoopState::PlanPatch,
            LoopEntryState::Verify => ReplLoopState::Verify,
            LoopEntryState::RetryDecision => ReplLoopState::RetryDecision,
        }
    }

    pub fn apply(
        &mut self,
        event: RepairLoopEvent,
    ) -> Result<StateTransition, TransitionError> {
        let from = self.state;
        let to = match (self.state, &event) {
            (ReplLoopState::Idle, RepairLoopEvent::PromptAccepted | RepairLoopEvent::SessionResumed) => {
                ReplLoopState::Analyze
            }
            (ReplLoopState::Analyze, RepairLoopEvent::AnalysisSucceeded) => ReplLoopState::PlanPatch,
            (ReplLoopState::Analyze, RepairLoopEvent::AnalysisAmbiguous) => ReplLoopState::Escalated,
            (ReplLoopState::Analyze, RepairLoopEvent::AnalysisUnsafe) => ReplLoopState::Escalated,
            (ReplLoopState::PlanPatch, RepairLoopEvent::PatchPlanned) => ReplLoopState::ApplySandbox,
            (ReplLoopState::PlanPatch, RepairLoopEvent::PatchUnsafe) => ReplLoopState::Escalated,
            (ReplLoopState::PlanPatch, RepairLoopEvent::NoViablePatch) => ReplLoopState::Escalated,
            (ReplLoopState::ApplySandbox, RepairLoopEvent::SandboxApplied) => ReplLoopState::Verify,
            (ReplLoopState::ApplySandbox, RepairLoopEvent::SandboxApplyFailed) => {
                ReplLoopState::RetryDecision
            }
            (ReplLoopState::Verify, RepairLoopEvent::VerifyPassed) => ReplLoopState::CommitDecision,
            (ReplLoopState::Verify, RepairLoopEvent::VerifyFailed) => {
                ReplLoopState::ClassifyFailure
            }
            (ReplLoopState::ClassifyFailure, RepairLoopEvent::FailureClassified(class)) => {
                self.previous_failure = Some(*class);
                ReplLoopState::RetryDecision
            }
            (ReplLoopState::ClassifyFailure, RepairLoopEvent::FailureUnknown) => {
                ReplLoopState::Escalated
            }
            (ReplLoopState::CommitDecision, RepairLoopEvent::CommitApproved) => {
                ReplLoopState::CommitLocal
            }
            (ReplLoopState::CommitDecision, RepairLoopEvent::CommitSkipped) => {
                ReplLoopState::Completed
            }
            (ReplLoopState::CommitLocal, RepairLoopEvent::CommitApproved) => {
                ReplLoopState::Completed
            }
            (ReplLoopState::CommitLocal, RepairLoopEvent::CommitFailed) => ReplLoopState::Rollback,
            (ReplLoopState::Rollback, RepairLoopEvent::RollbackSucceeded) => {
                ReplLoopState::Escalated
            }
            _ => {
                return Err(TransitionError::InvalidTransition { state: self.state, event });
            }
        };

        self.state = to;
        Ok(StateTransition { from, event, to })
    }

    pub fn retry_outcome(
        &self,
        attempts: u8,
        confidence: f32,
        no_op_count: u8,
        current_failure: FailureClass,
        improvement_detected: bool,
    ) -> ReplLoopState {
        match RetryEvaluator::decide(
            self.policy,
            attempts,
            confidence,
            no_op_count,
            current_failure,
            self.previous_failure,
            improvement_detected,
        ) {
            RetryDecision::Rollback => ReplLoopState::Rollback,
            RetryDecision::Escalate(_) => ReplLoopState::Escalated,
            RetryDecision::ChangeStrategy | RetryDecision::Replan => ReplLoopState::PlanPatch,
        }
    }

    pub fn commit_outcome(
        &self,
        decision: CommitGuardDecision,
    ) -> ReplLoopState {
        match decision {
            CommitGuardDecision::Allow { .. } => ReplLoopState::CommitLocal,
            CommitGuardDecision::Reject { .. } => ReplLoopState::Completed,
        }
    }

    pub fn status(&self) -> RepairLoopStatus {
        let stop_reason = match self.state {
            ReplLoopState::Escalated => Some(EscalationReason::UnknownCompilerFailure),
            _ => None,
        };
        RepairLoopStatus {
            state: self.state,
            stop_reason,
        }
    }

    pub fn evaluate_commit(
        &self,
        context: &super::state::CommitDecisionContext,
    ) -> ReplLoopState {
        self.commit_outcome(CommitGuard::evaluate(context))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nl::r#loop::{CommitDecisionContext, LoopOrigin, RepairLoopContext};

    fn sample_context() -> RepairLoopContext {
        RepairLoopContext {
            target: Some(PathBuf::from("apps/cli/src/repl.rs")),
            logical_node: Some("determinism".to_string()),
            changed_files: vec![PathBuf::from("apps/cli/src/repl.rs")],
            diagnostics: vec![String::from("error[E0432]")],
            rollback_token: Some("rb-1".to_string()),
            previous_strategy: None,
            origin: LoopOrigin::Analyze,
        }
    }

    #[test]
    fn happy_path_reaches_commit_decision() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        controller.apply(RepairLoopEvent::PromptAccepted).unwrap();
        controller.apply(RepairLoopEvent::AnalysisSucceeded).unwrap();
        controller.apply(RepairLoopEvent::PatchPlanned).unwrap();
        controller.apply(RepairLoopEvent::SandboxApplied).unwrap();
        controller.apply(RepairLoopEvent::VerifyPassed).unwrap();
        assert_eq!(controller.state(), ReplLoopState::CommitDecision);
    }

    #[test]
    fn repeated_failure_changes_to_replan_or_rollback_deterministically() {
        let controller = RepairLoopController::new(RetryPolicy::default());
        assert_eq!(
            controller.retry_outcome(2, 0.9, 0, FailureClass::CompileError, false),
            ReplLoopState::Rollback
        );
    }

    #[test]
    fn protected_branch_skips_commit() {
        let controller = RepairLoopController::new(RetryPolicy::default());
        let next = controller.evaluate_commit(&CommitDecisionContext {
            branch_name: "main".to_string(),
            changed_files: vec![PathBuf::from("apps/cli/src/nl/loop/transition.rs")],
            explicit_confirmation: true,
            diff_preview_ready: true,
        });
        assert_eq!(next, ReplLoopState::Completed);
    }

    #[test]
    fn analyze_entry_starts_at_analyze() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = controller
            .start_from(sample_context(), LoopEntryState::Analyze)
            .unwrap();
        assert_eq!(outcome.status.state, ReplLoopState::Analyze);
        assert_eq!(controller.state(), ReplLoopState::Analyze);
    }

    #[test]
    fn plan_patch_entry_starts_at_plan_patch() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = controller
            .start_from(sample_context(), LoopEntryState::PlanPatch)
            .unwrap();
        assert_eq!(outcome.status.state, ReplLoopState::PlanPatch);
        assert_eq!(controller.state(), ReplLoopState::PlanPatch);
    }

    #[test]
    fn verify_entry_starts_at_verify() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = controller
            .start_from(sample_context(), LoopEntryState::Verify)
            .unwrap();
        assert_eq!(outcome.status.state, ReplLoopState::Verify);
        assert_eq!(controller.state(), ReplLoopState::Verify);
    }

    #[test]
    fn retry_decision_entry_starts_at_retry_decision() {
        let mut controller = RepairLoopController::new(RetryPolicy::default());
        let outcome = controller
            .start_from(sample_context(), LoopEntryState::RetryDecision)
            .unwrap();
        assert_eq!(outcome.status.state, ReplLoopState::RetryDecision);
        assert_eq!(controller.state(), ReplLoopState::RetryDecision);
    }

    #[test]
    fn start_wrapper_matches_analyze_entry() {
        let mut direct = RepairLoopController::new(RetryPolicy::default());
        let mut wrapped = RepairLoopController::new(RetryPolicy::default());
        let left = direct
            .start_from(sample_context(), LoopEntryState::Analyze)
            .unwrap();
        let right = wrapped.start(sample_context()).unwrap();
        assert_eq!(left, right);
        assert_eq!(wrapped.state(), ReplLoopState::Analyze);
    }
}
