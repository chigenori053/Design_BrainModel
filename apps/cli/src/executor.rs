/// Phase 2: IR-driven Executor
///
/// `IrExecutor` trait defines the contract for step execution.
/// Callers emit IR lifecycle events; the executor only computes results.
use crate::command::CommandRegistry;
use crate::ir::{ArtifactRef, ExecutionStatus, MemoryContext};
use crate::nl::types::PlannedStep;
use crate::plan::Plan;
use crate::session::AgentSession;

// ── IrExecutor trait ──────────────────────────────────────────────────────────

/// Output of a single step execution.
/// Does NOT contain IR event emission — the caller is responsible for that.
#[derive(Debug, Clone)]
pub struct StepExecutionOutput {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub status: ExecutionStatus,
    pub artifacts: Vec<ArtifactRef>,
}

impl StepExecutionOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            stdout: Some(stdout.into()),
            stderr: None,
            status: ExecutionStatus::Success,
            artifacts: Vec::new(),
        }
    }

    pub fn failure(stderr: impl Into<String>) -> Self {
        Self {
            stdout: None,
            stderr: Some(stderr.into()),
            status: ExecutionStatus::Failure,
            artifacts: Vec::new(),
        }
    }

    pub fn skipped() -> Self {
        Self {
            stdout: None,
            stderr: None,
            status: ExecutionStatus::Skipped,
            artifacts: Vec::new(),
        }
    }
}

/// Execution contract: compute a result from a step.
///
/// MUST NOT write IR events — the caller wraps this in step lifecycle emission.
pub trait IrExecutor {
    fn execute_step(&mut self, step: &PlannedStep, context: &MemoryContext) -> StepExecutionOutput;
}

// ─────────────────────────────────────────────────────────────────────────────

pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Plan を逐次実行する
    ///
    /// 各ステップを順に実行し、結果メッセージのリストを返す。
    /// いずれかのステップが失敗した場合はそこで停止する。
    #[deprecated(note = "Use execute_ir_plan instead")]
    pub fn execute(
        &self,
        _plan: &mut Plan,
        _session: &mut AgentSession,
        _registry: &CommandRegistry,
    ) -> Vec<String> {
        panic!("Legacy executor is disabled. Use IR execution.");
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::command::CommandRegistry;
    use crate::commands::register_defaults;
    use crate::plan::{CommandInvocation, Step};
    use crate::session::AgentSession;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        registry
    }

    #[test]
    #[should_panic(expected = "Legacy executor is disabled. Use IR execution.")]
    fn executor_is_disabled_for_single_step() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![Step::new(
            0,
            "Generate spec",
            Some(CommandInvocation::new("generate", Some("spec"), &["api"])),
        )];
        let mut plan = crate::plan::Plan::new("p1", steps);

        let _ = executor.execute(&mut plan, &mut session, &registry);
    }

    #[test]
    #[should_panic(expected = "Legacy executor is disabled. Use IR execution.")]
    fn executor_is_disabled_for_unknown_command() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![Step::new(
            0,
            "Unknown",
            Some(CommandInvocation::new("nonexistent", None, &[])),
        )];
        let mut plan = crate::plan::Plan::new("p2", steps);

        let _ = executor.execute(&mut plan, &mut session, &registry);
    }

    #[test]
    #[should_panic(expected = "Legacy executor is disabled. Use IR execution.")]
    fn executor_is_disabled_for_multiple_steps() {
        let registry = build_registry();
        let executor = Executor::new();
        let mut session = AgentSession::new();

        let steps = vec![
            Step::new(
                0,
                "Bad step",
                Some(CommandInvocation::new("bad", None, &[])),
            ),
            Step::new(
                1,
                "Good step",
                Some(CommandInvocation::new("generate", Some("spec"), &["x"])),
            ),
        ];
        let mut plan = crate::plan::Plan::new("p3", steps);

        let _ = executor.execute(&mut plan, &mut session, &registry);
    }
}
