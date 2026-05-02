use crate::failure::FailureContext;
use crate::types::{CodeIrProgram, RunResult};
use execution_hardening::Checksum;

/// Records of all prior executions within a strategy session.
///
/// Used for:
/// - Avoiding re-trying plans that have already failed with the same error.
/// - Preferring repair strategies that match patterns seen in successful runs.
///
/// Spec §11 ExecutionHistory
#[derive(Debug, Clone, Default)]
pub struct ExecutionHistory {
    /// Records of every execution attempt in chronological order.
    pub traces: Vec<HistoryEntry>,
    /// All failure contexts collected across attempts.
    pub failures: Vec<FailureContext>,
}

/// A single history record.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Checksum of the plan that was run.
    pub plan_checksum: Checksum,
    /// Whether it succeeded.
    pub success: bool,
    /// Combined stdout/stderr for reference.
    pub stdout: String,
    pub stderr: String,
}

impl ExecutionHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful run.
    pub fn add_success(&mut self, plan: &CodeIrProgram, result: &RunResult) {
        self.traces.push(HistoryEntry {
            plan_checksum: plan_checksum(plan),
            success: true,
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
        });
    }

    /// Record a failed run and its failure context.
    pub fn add_failure(
        &mut self,
        failure: FailureContext,
        plan: &CodeIrProgram,
        result: &RunResult,
    ) {
        self.traces.push(HistoryEntry {
            plan_checksum: plan_checksum(plan),
            success: false,
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
        });
        self.failures.push(failure);
    }

    /// Returns `true` if we have already attempted `plan` and it failed.
    ///
    /// Used by the planner to avoid re-submitting a known-bad plan.
    /// Spec §11.2: 同一失敗の再試行回避
    pub fn has_failed(&self, plan: &CodeIrProgram) -> bool {
        let cs = plan_checksum(plan);
        self.traces
            .iter()
            .any(|e| e.plan_checksum == cs && !e.success)
    }

    /// Returns `true` if an identical plan has already succeeded.
    pub fn has_succeeded(&self, plan: &CodeIrProgram) -> bool {
        let cs = plan_checksum(plan);
        self.traces
            .iter()
            .any(|e| e.plan_checksum == cs && e.success)
    }

    /// Number of total attempts recorded.
    pub fn attempt_count(&self) -> usize {
        self.traces.len()
    }

    /// Number of recorded failures.
    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }

    /// Most recent failure, if any.
    pub fn last_failure(&self) -> Option<&FailureContext> {
        self.failures.last()
    }
}

/// Compute a stable checksum for an `ExecutionPlan`.
///
/// Mirrors `compute_plan_checksum` in `hardened_controller.rs` but
/// lives here so that `ExecutionHistory` can do plan deduplication
/// without depending on the controller internals.
pub(crate) fn plan_checksum(plan: &CodeIrProgram) -> Checksum {
    use execution_hardening::ChecksumBuilder;

    let lang = format!("{:?}", plan.language);
    let fw = plan.framework.as_deref().unwrap_or("");
    let root = plan.project_root.to_string_lossy();

    let mut b = ChecksumBuilder::new()
        .update_str(&lang)
        .update_str(fw)
        .update_str(&root);

    for cmd in &plan.dependency_plan.install_commands {
        b = b.update_str(cmd);
    }
    for cmd in &plan.build_plan.build_commands {
        b = b.update_str(cmd);
    }
    for cmd in &plan.run_plan.run_commands {
        b = b.update_str(cmd);
    }
    for cmd in &plan.test_plan.test_commands {
        b = b.update_str(cmd);
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use execution_core::engine::execution_plan::*;
    use std::path::PathBuf;

    fn empty_plan() -> ExecutionPlan {
        ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec![],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: execution_core::engine::execution_plan::TestPlan {
                test_files: vec![],
                test_commands: vec![],
            },
        }
    }

    fn ok_result() -> RunResult {
        RunResult {
            success: true,
            failure_type: None,
            stdout: "ok".into(),
            stderr: String::new(),
            steps: vec![],
        }
    }

    #[test]
    fn records_success() {
        let mut h = ExecutionHistory::new();
        let plan = empty_plan();
        h.add_success(&plan, &ok_result());
        assert_eq!(h.attempt_count(), 1);
        assert!(h.has_succeeded(&plan));
        assert!(!h.has_failed(&plan));
    }

    #[test]
    fn deduplication_by_checksum() {
        let mut h = ExecutionHistory::new();
        let plan = empty_plan();
        let fail_result = RunResult {
            success: false,
            failure_type: Some(
                execution_stability_core::failure::failure_type::FailureType::BuildFailure,
            ),
            stdout: String::new(),
            stderr: "err".into(),
            steps: vec![],
        };
        use crate::failure::{FailureContext, FailureKind, StepId, StepInput};
        let fc = FailureContext {
            step_id: StepId::new("build", 0),
            error: FailureKind::ExecutionError {
                phase: "build".into(),
            },
            input: StepInput {
                command: vec![],
                phase: "build".into(),
            },
            output: None,
        };
        h.add_failure(fc, &plan, &fail_result);
        assert!(h.has_failed(&plan));
        assert!(!h.has_succeeded(&plan));
    }
}
