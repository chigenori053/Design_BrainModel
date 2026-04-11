pub mod commit_guard;
pub mod failure_classifier;
pub mod promotion;
pub mod retry_policy;
pub mod state;
pub mod trajectory_memory;
pub mod transition;
pub mod verify_router;

pub use commit_guard::{BranchSafety, CommitGuard, CommitGuardDecision};
pub use failure_classifier::{FailureClassifier, FailureClassifierInput};
pub use promotion::{LoopOrigin, LoopPromotable, PromotionError, PromotionGuard, RepairLoopContext};
pub use retry_policy::{ConfidencePolicy, RetryBudget, RetryDecision, RetryEvaluator};
pub use state::{
    AnalyzeContext, AnalyzeResult, CommitDecisionContext, EscalationReason, FailureClass,
    LoopEntryState, PatchPlan, PatchStrategy, RepairLoopSnapshot, RepairTrajectory,
    ReplLoopState, RetryPolicy, SandboxApplyResult, VerifyResult,
};
pub use trajectory_memory::{InMemoryTrajectoryStore, TrajectoryStore};
pub use transition::{
    LoopOutcome, RepairLoopController, RepairLoopEvent, RepairLoopStatus, StateTransition,
    TransitionError,
};
pub use verify_router::{VerificationRoute, VerifyRouter};
