use serde::{Deserialize, Serialize};

/// A complete, auditable record of a single execution step.
///
/// Satisfies spec §8.1 必須ログ:
/// - step_index   — ordinal position in the execution sequence
/// - execution_op — human-readable operation name (phase name + command)
/// - input        — what was fed to the step (command + args + env context)
/// - output       — what the step produced (stdout, exit code)
/// - effect       — side-effects staged or committed by this step
/// - timestamp    — Unix epoch milliseconds at step start
///
/// Spec §8.2: JSONL format (one JSON object per line, deterministic field order)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardenedStepTrace {
    /// Zero-based index within the containing `ExecutionTrace`.
    pub step_index: usize,
    /// Operation label, e.g. `"build:cargo build --release"`.
    pub execution_op: String,
    /// Serialized input context for this step.
    pub input: StepInput,
    /// Captured output from the step.
    pub output: StepOutput,
    /// Effects staged or committed during this step.
    pub effect: StepEffect,
    /// Unix epoch milliseconds when the step started.
    pub timestamp_ms: u64,
    /// Unix epoch milliseconds when the step ended.
    pub end_timestamp_ms: u64,
    /// Whether the step succeeded.
    pub success: bool,
}

/// Input context for one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepInput {
    /// Pre-split command vector (binary + args).
    pub command: Vec<String>,
    /// Phase name (e.g. `"build"`, `"run"`, `"test"`).
    pub phase: String,
}

/// Output produced by one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOutput {
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error (may be empty in fully sandboxed mode).
    pub stderr: String,
    /// Process exit code.
    pub exit_code: Option<i32>,
}

/// Side-effects recorded during one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepEffect {
    /// Stable keys of effects staged during this step.
    pub staged_effect_keys: Vec<String>,
    /// Stable keys of effects committed during this step.
    pub committed_effect_keys: Vec<String>,
}

impl HardenedStepTrace {
    /// Build a `HardenedStepTrace` for a step that has already completed.
    pub fn new(
        step_index: usize,
        phase: impl Into<String>,
        command: Vec<String>,
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        success: bool,
        timestamp_ms: u64,
        end_timestamp_ms: u64,
        staged_effect_keys: Vec<String>,
        committed_effect_keys: Vec<String>,
    ) -> Self {
        let phase = phase.into();
        let execution_op = if command.is_empty() {
            phase.clone()
        } else {
            format!("{phase}:{}", command.join(" "))
        };
        Self {
            step_index,
            execution_op,
            input: StepInput { command, phase },
            output: StepOutput {
                stdout,
                stderr,
                exit_code,
            },
            effect: StepEffect {
                staged_effect_keys,
                committed_effect_keys,
            },
            timestamp_ms,
            end_timestamp_ms,
            success,
        }
    }
}
