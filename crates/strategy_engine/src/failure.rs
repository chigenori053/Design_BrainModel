use crate::types::{RunResult, StepInfo};
use execution_stability_core::failure::failure_type::FailureType;

// ── FailureKind ───────────────────────────────────────────────────────────────

/// Unified failure taxonomy for Phase D.
///
/// Extends Phase C failure types (spec §6.1) to include Phase C.5 hardening
/// violations and Phase D-level validation errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FailureKind {
    // ── Phase C ──────────────────────────────────────────────────────────────
    /// The plan itself was structurally invalid before execution.
    ValidationError,
    /// A command exited with a non-zero status during execution.
    ExecutionError { phase: String },
    /// A step exceeded its allocated time budget.
    Timeout { phase: String },
    /// An environmental prerequisite was absent or misconfigured.
    EnvironmentError,

    // ── Phase C.5 ────────────────────────────────────────────────────────────
    /// A sandboxed command violated isolation constraints.
    SafetyViolation,
    /// A plan or output checksum did not match the expected value.
    ChecksumMismatch,
    /// Execution state became inconsistent.
    StateCorruption,
    /// A sandboxed command tried to break out of the sandbox.
    SandboxViolation,
    /// A replay run diverged from the original trace.
    TraceMismatch,
}

impl FailureKind {
    /// Whether this failure is recoverable by retry or repair.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ExecutionError { .. }
                | Self::Timeout { .. }
                | Self::EnvironmentError
                | Self::ValidationError
        )
    }

    /// Whether this failure is a hard safety violation that should abort.
    pub fn is_safety_violation(&self) -> bool {
        matches!(
            self,
            Self::SafetyViolation
                | Self::ChecksumMismatch
                | Self::StateCorruption
                | Self::SandboxViolation
                | Self::TraceMismatch
        )
    }
}

impl From<FailureType> for FailureKind {
    fn from(ft: FailureType) -> Self {
        match ft {
            FailureType::DependencyFailure => Self::ExecutionError {
                phase: "dependency".to_string(),
            },
            FailureType::BuildFailure => Self::ExecutionError {
                phase: "build".to_string(),
            },
            FailureType::RuntimeFailure => Self::ExecutionError {
                phase: "run".to_string(),
            },
            FailureType::TestFailure => Self::ExecutionError {
                phase: "test".to_string(),
            },
            FailureType::Timeout => Self::Timeout {
                phase: "unknown".to_string(),
            },
            FailureType::EnvironmentError => Self::EnvironmentError,
            FailureType::StateCorruption => Self::StateCorruption,
            FailureType::SandboxViolation => Self::SandboxViolation,
            FailureType::TraceMismatch => Self::TraceMismatch,
            FailureType::ChecksumMismatch => Self::ChecksumMismatch,
        }
    }
}

// ── StepId ────────────────────────────────────────────────────────────────────

/// Identifies the step within an execution where a failure occurred.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StepId {
    /// Phase name: "dependency", "build", "run", "test".
    pub phase: String,
    /// Zero-based index within the phase's command list.
    pub command_index: usize,
}

impl StepId {
    pub fn new(phase: impl Into<String>, command_index: usize) -> Self {
        Self {
            phase: phase.into(),
            command_index,
        }
    }

    pub fn phase(&self) -> &str {
        &self.phase
    }
}

// ── FailureContext ─────────────────────────────────────────────────────────────

/// Full context of a single execution failure.  Spec §6.2 FailureContext.
#[derive(Debug, Clone)]
pub struct FailureContext {
    /// Which step failed.
    pub step_id: StepId,
    /// Classified failure kind.
    pub error: FailureKind,
    /// Input that was fed to the failing step.
    pub input: StepInput,
    /// Output captured from the failing step (if any).
    pub output: Option<StepOutput>,
}

/// Input side of a failed step.
#[derive(Debug, Clone)]
pub struct StepInput {
    pub command: Vec<String>,
    pub phase: String,
}

/// Output side of a failed step.
#[derive(Debug, Clone)]
pub struct StepOutput {
    pub stdout: String,
    pub stderr: String,
}

// ── StrategyFailureAnalyzer ───────────────────────────────────────────────────

/// Analyses a `RunResult` and produces a `FailureContext` for Phase D planning.
///
/// Spec §8.1: 失敗 → FailureAnalyzer
#[derive(Debug, Default)]
pub struct StrategyFailureAnalyzer;

impl StrategyFailureAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Extract the primary `FailureContext` from a failed `RunResult`.
    pub fn analyze(&self, result: &RunResult) -> Option<FailureContext> {
        if result.success {
            return None;
        }

        let ft = result.failure_type.clone()?;
        let kind = FailureKind::from(ft);

        // Find the first failed step
        let (step_id, step_info) = self.first_failed_step(&result.steps, &kind);

        let input = StepInput {
            command: vec![step_id.phase.clone()],
            phase: step_id.phase.clone(),
        };
        let output = step_info.map(|s| StepOutput {
            stdout: s.stdout.clone(),
            stderr: s.stderr.clone(),
        });

        Some(FailureContext {
            step_id,
            error: kind,
            input,
            output,
        })
    }

    fn first_failed_step<'a>(
        &self,
        steps: &'a [StepInfo],
        kind: &FailureKind,
    ) -> (StepId, Option<&'a StepInfo>) {
        // Infer from FailureKind
        let phase = match kind {
            FailureKind::ExecutionError { phase } | FailureKind::Timeout { phase } => phase.clone(),
            _ => "unknown".to_string(),
        };

        let failed = steps.iter().enumerate().find(|(_, s)| !s.success);
        match failed {
            Some((idx, step)) => (StepId::new(&step.phase, idx), Some(step)),
            None => (StepId::new(phase, 0), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_failed_result(phase: &str) -> RunResult {
        RunResult {
            success: false,
            failure_type: Some(FailureType::BuildFailure),
            stdout: String::new(),
            stderr: "build error".to_string(),
            steps: vec![StepInfo {
                phase: phase.to_string(),
                success: false,
                stdout: String::new(),
                stderr: "build error".to_string(),
            }],
        }
    }

    #[test]
    fn analyze_returns_none_on_success() {
        let analyzer = StrategyFailureAnalyzer::new();
        let result = RunResult {
            success: true,
            failure_type: None,
            stdout: "ok".into(),
            stderr: String::new(),
            steps: vec![],
        };
        assert!(analyzer.analyze(&result).is_none());
    }

    #[test]
    fn analyze_extracts_build_failure() {
        let analyzer = StrategyFailureAnalyzer::new();
        let result = make_failed_result("build");
        let ctx = analyzer.analyze(&result).unwrap();
        assert!(matches!(ctx.error, FailureKind::ExecutionError { .. }));
        assert_eq!(ctx.step_id.phase, "build");
    }

    #[test]
    fn failure_kind_is_recoverable() {
        assert!(
            FailureKind::ExecutionError {
                phase: "build".into()
            }
            .is_recoverable()
        );
        assert!(!FailureKind::SandboxViolation.is_recoverable());
        assert!(!FailureKind::StateCorruption.is_recoverable());
    }
}
