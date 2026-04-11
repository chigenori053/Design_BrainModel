use std::path::PathBuf;

use anyhow::Result;

use super::promotion::{
    LoopOrigin, LoopPromotable, PromotionError, PromotionGuard, RepairLoopContext,
};
use super::retry_policy::RetryEvaluator;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplLoopState {
    Idle,
    Analyze,
    PlanPatch,
    ApplySandbox,
    Verify,
    ClassifyFailure,
    RetryDecision,
    CommitDecision,
    CommitLocal,
    Rollback,
    Completed,
    Escalated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopEntryState {
    Analyze,
    PlanPatch,
    Verify,
    RetryDecision,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RepairLoopSnapshot {
    pub target: Option<PathBuf>,
    pub logical_node: Option<String>,
    pub last_strategy: Option<PatchStrategy>,
    pub attempts: u8,
    pub no_op_count: u8,
    pub failure_signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AnalyzeContext {
    pub target: Option<PathBuf>,
    pub logical_node: Option<String>,
    pub previous_state: Option<RepairLoopSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalyzeResult {
    pub target: PathBuf,
    pub affected_crates: Vec<String>,
    pub confidence: f32,
    pub logical_node: Option<String>,
    pub ambiguous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchStrategy {
    ImportRebind,
    TraitExtraction,
    CycleCut,
    VisibilityFix,
    LifetimeNarrowing,
    BorrowScopeShrink,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatchPlan {
    pub strategy: PatchStrategy,
    pub estimated_files: Vec<PathBuf>,
    pub expected_diff_entropy: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxApplyResult {
    pub applied: bool,
    pub changed_files: Vec<PathBuf>,
    pub rollback_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyResult {
    pub success: bool,
    pub diagnostics: Vec<String>,
    pub verification_cost_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    CompileError,
    TestFailure,
    MissingImport,
    TraitMismatch,
    LifetimeConflict,
    BorrowConflict,
    NoImprovement,
    UnsafeExpansion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscalationReason {
    AmbiguousTarget,
    UnsafeDiff,
    RetryBudgetExceeded,
    ConfidenceCollapsed,
    UnknownCompilerFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub confidence_floor_milli: u16,
    pub no_op_limit: u8,
}

impl RetryPolicy {
    pub fn confidence_floor(self) -> f32 {
        f32::from(self.confidence_floor_milli) / 1000.0
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            confidence_floor_milli: 700,
            no_op_limit: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitDecisionContext {
    pub branch_name: String,
    pub changed_files: Vec<PathBuf>,
    pub explicit_confirmation: bool,
    pub diff_preview_ready: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepairTrajectory {
    pub failure_signature: String,
    pub patch_strategy: PatchStrategy,
    pub target_shape: String,
    pub converged: bool,
    pub recall_confidence: f32,
}

impl LoopPromotable for AnalyzeResult {
    fn promote(self) -> Result<RepairLoopContext> {
        if self.ambiguous
            || self.target.as_os_str().is_empty()
            || self.confidence
                < RetryEvaluator::confidence_policy_for_origin(LoopOrigin::Analyze)
                    .promote_threshold
        {
            return Err(PromotionError::AmbiguousTarget.into());
        }

        Ok(RepairLoopContext {
            target: Some(self.target),
            logical_node: self.logical_node,
            changed_files: Vec::new(),
            diagnostics: Vec::new(),
            rollback_token: None,
            previous_strategy: None,
            origin: LoopOrigin::Analyze,
        })
    }
}

impl LoopPromotable for RepairTrajectory {
    fn promote(self) -> Result<RepairLoopContext> {
        let promote_threshold =
            RetryEvaluator::confidence_policy_for_origin(LoopOrigin::MemoryRecall)
                .promote_threshold;
        if self.recall_confidence < promote_threshold
            || self.recall_confidence < PromotionGuard::default().min_memory_confidence
        {
            return Err(PromotionError::LowRecallConfidence.into());
        }

        Ok(RepairLoopContext {
            target: None,
            logical_node: Some(self.target_shape),
            changed_files: Vec::new(),
            diagnostics: Vec::new(),
            rollback_token: None,
            previous_strategy: Some(self.patch_strategy),
            origin: LoopOrigin::MemoryRecall,
        })
    }
}

#[cfg(test)]
mod promotion_tests {
    use super::*;
    use crate::nl::r#loop::{LoopEntryState, LoopPromotable};

    #[test]
    fn analyze_result_promotes_to_plan_patch_context() {
        let context = AnalyzeResult {
            target: PathBuf::from("apps/cli/src/repl.rs"),
            affected_crates: vec![String::from("design_cli")],
            confidence: 0.92,
            logical_node: None,
            ambiguous: false,
        }
        .promote()
        .expect("analyze promotion should succeed");
        assert_eq!(context.origin, LoopOrigin::Analyze);
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::PlanPatch
        );
    }

    #[test]
    fn analyze_result_rejects_ambiguous_target() {
        let error = AnalyzeResult {
            target: PathBuf::from("apps/cli/src"),
            affected_crates: vec![String::from("design_cli")],
            confidence: 0.4,
            logical_node: None,
            ambiguous: true,
        }
        .promote()
        .expect_err("ambiguous analyze result must fail");
        assert!(error.to_string().contains("ambiguous target"));
    }

    #[test]
    fn analyze_result_preserves_logical_node() {
        let context = AnalyzeResult {
            target: PathBuf::from("apps/cli/src/repl.rs"),
            affected_crates: vec![String::from("design_cli")],
            confidence: 0.9,
            logical_node: Some("determinism".to_string()),
            ambiguous: false,
        }
        .promote()
        .expect("analyze promotion should succeed");
        assert_eq!(context.logical_node.as_deref(), Some("determinism"));
    }

    #[test]
    fn memory_trajectory_promotes_to_plan_patch() {
        let context = RepairTrajectory {
            failure_signature: "E0502".to_string(),
            patch_strategy: PatchStrategy::BorrowScopeShrink,
            target_shape: "replay".to_string(),
            converged: true,
            recall_confidence: 0.95,
        }
        .promote()
        .expect("memory promotion should succeed");
        assert_eq!(context.origin, LoopOrigin::MemoryRecall);
        assert_eq!(context.previous_strategy, Some(PatchStrategy::BorrowScopeShrink));
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::PlanPatch
        );
    }

    #[test]
    fn memory_trajectory_rejects_low_confidence() {
        let error = RepairTrajectory {
            failure_signature: "E0432".to_string(),
            patch_strategy: PatchStrategy::ImportRebind,
            target_shape: "adapter-world edge".to_string(),
            converged: true,
            recall_confidence: 0.4,
        }
        .promote()
        .expect_err("low-confidence recall must fail");
        assert!(error.to_string().contains("low recall confidence"));
    }
}
