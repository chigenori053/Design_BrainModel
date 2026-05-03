use execution_core::engine::execution_plan::ExecutionPlan;
use execution_hardening::SandboxedCommand;
use std::path::{Component, Path, PathBuf};

use crate::convergence::ExecutionOp;

pub const FIXED_GIT_COMMIT_MESSAGE: &str = "auto fix";

// ── CodeIrProgram ─────────────────────────────────────────────────────────────

/// The execution-level IR program: a concrete, validated `ExecutionPlan`.
///
/// Phase D operates on `CodeIrProgram` as the unit of planning.
/// Repair and replan operations produce new `CodeIrProgram` values;
/// the runner executes them via `RunIntegrator`.
pub type CodeIrProgram = ExecutionPlan;

// ── Intent ────────────────────────────────────────────────────────────────────

/// The user's original execution intent — the semantic goal that the strategy
/// engine must satisfy, independent of which concrete plan achieves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    /// Human-readable description of the goal.
    pub description: String,
    /// Parsed action used for clarification gating.
    pub action: Action,
    /// Module-level target such as `parser`, `auth`, or `db`.
    pub target: Option<String>,
    /// Explicit file path, for example `parser.rs`.
    pub file: Option<String>,
    /// Explicit symbol, for example `parse_input function`.
    pub symbol: Option<String>,
    /// Hard constraints the result must satisfy.
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Fix,
    Improve,
    Optimize,
    RefactorGeneric,
    Build,
    Run,
    Test,
    Other(String),
}

impl Intent {
    pub fn new(description: impl Into<String>) -> Self {
        let description = description.into();
        let action = parse_action(&description);
        let file = extract_file(&description);
        let symbol = extract_symbol(&description, file.as_deref());
        let target =
            extract_module_target(&description, &action, file.as_deref(), symbol.as_deref());
        Self {
            description,
            action,
            target,
            file,
            symbol,
            constraints: Vec::new(),
        }
    }

    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }
}

fn parse_action(description: &str) -> Action {
    let lower = description.to_ascii_lowercase();
    if has_word(&lower, "fix") {
        Action::Fix
    } else if has_word(&lower, "improve") {
        Action::Improve
    } else if has_word(&lower, "optimize") {
        Action::Optimize
    } else if has_word(&lower, "refactor") {
        Action::RefactorGeneric
    } else if has_word(&lower, "build") {
        Action::Build
    } else if has_word(&lower, "run") {
        Action::Run
    } else if has_word(&lower, "test") {
        Action::Test
    } else {
        Action::Other(description.trim().to_string())
    }
}

fn extract_file(description: &str) -> Option<String> {
    description.split_whitespace().find_map(|token| {
        let trimmed = trim_intent_token(token);
        let lower = trimmed.to_ascii_lowercase();
        let is_file = [
            ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".kt", ".swift", ".c",
            ".cc", ".cpp", ".h", ".hpp", ".toml", ".json", ".yaml", ".yml", ".md",
        ]
        .iter()
        .any(|extension| lower.ends_with(extension));
        if is_file {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn extract_symbol(description: &str, file: Option<&str>) -> Option<String> {
    let tokens = normalized_tokens(description);
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            token.as_str(),
            "function" | "fn" | "struct" | "method" | "symbol"
        ) && index > 0
        {
            let candidate = &tokens[index - 1];
            if !is_noise_token(candidate) && Some(candidate.as_str()) != file {
                return Some(candidate.clone());
            }
        }
    }

    tokens
        .into_iter()
        .find(|token| token.contains('_') && !is_noise_token(token) && Some(token.as_str()) != file)
}

fn extract_module_target(
    description: &str,
    action: &Action,
    file: Option<&str>,
    symbol: Option<&str>,
) -> Option<String> {
    if !matches!(
        action,
        Action::Fix | Action::Improve | Action::Optimize | Action::RefactorGeneric
    ) {
        return None;
    }

    normalized_tokens(description).into_iter().find(|token| {
        !is_noise_token(token) && Some(token.as_str()) != file && Some(token.as_str()) != symbol
    })
}

fn normalized_tokens(description: &str) -> Vec<String> {
    description
        .split_whitespace()
        .map(trim_intent_token)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn trim_intent_token(token: &str) -> &str {
    token.trim_matches(|c: char| {
        !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-' && c != '/'
    })
}

fn has_word(description: &str, word: &str) -> bool {
    description
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == word)
}

fn is_noise_token(token: &str) -> bool {
    matches!(
        token,
        "fix"
            | "improve"
            | "optimize"
            | "refactor"
            | "build"
            | "run"
            | "test"
            | "bug"
            | "issue"
            | "problem"
            | "code"
            | "please"
            | "the"
            | "a"
            | "an"
            | "in"
            | "on"
            | "for"
            | "function"
            | "fn"
            | "struct"
            | "method"
            | "symbol"
    )
}

// ── ExecutionContext ──────────────────────────────────────────────────────────

/// Runtime context passed to the strategy engine.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Enable deterministic strategy selection (same input → same strategy).
    /// Spec §13.1: Determinismモード ON
    pub deterministic: bool,
    /// Per-execution overall timeout (ms).  `0` means no timeout.
    pub timeout_ms: u64,
    /// Fixed repository root used by typed Git operations.
    pub repo_root: PathBuf,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            deterministic: true,
            timeout_ms: 0,
            repo_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

// ── ExecutionMode ─────────────────────────────────────────────────────────────

/// Controls whether strategy exploration (retry/repair/replan) is active.
///
/// Spec DBM-EXPLOSION-FIX-TIER1-SPEC §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Proposal mode: no retry, repair, or replan.  Candidates are sampled
    /// once from heuristics and capped at `MAX_CANDIDATES`.  Spec §2.1.
    Proposal,
    /// Full execution mode: retry, repair, and replan are all permitted.
    Execution,
}

// ── StrategyInput / StrategyOutput ───────────────────────────────────────────

/// All inputs to a strategy engine run.  Spec §5.1.
#[derive(Debug, Clone)]
pub struct StrategyInput {
    /// The semantic goal to achieve.
    pub intent: Intent,
    /// The initial execution plan (may be repaired or replaced).
    pub initial_plan: CodeIrProgram,
    /// Runtime context (determinism, timeout, etc.).
    pub context: ExecutionContext,
    /// Execution history from prior runs (used for optimization).
    pub history: crate::history::ExecutionHistory,
}

/// The result of a complete strategy engine run.  Spec §5.2.
#[derive(Debug)]
pub struct StrategyOutput {
    /// The plan that was eventually accepted (last attempted plan).
    pub selected_plan: CodeIrProgram,
    /// Full strategy trace: all attempts, their outcomes, and the final decision.
    pub strategy_trace: crate::trace::StrategyTrace,
    /// Whether the strategy engine ultimately succeeded.
    pub success: bool,
}

// ── RunResult ─────────────────────────────────────────────────────────────────

/// Normalised result from a single plan execution.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub success: bool,
    pub failure_type: Option<execution_stability_core::failure::failure_type::FailureType>,
    /// Combined stdout from all phases.
    pub stdout: String,
    /// Combined stderr from all phases.
    pub stderr: String,
    /// Per-phase step summaries (phase name, success, output).
    pub steps: Vec<StepInfo>,
}

/// Summary of a single execution phase.
#[derive(Debug, Clone)]
pub struct StepInfo {
    pub phase: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

// ── RunIntegrator trait ───────────────────────────────────────────────────────

/// Abstraction over the Phase C / C.5 execution layer.
///
/// The strategy engine calls `run()` and receives a normalised `RunResult`.
/// This trait is the boundary between Phase D (strategy) and Phase C (execution).
pub trait RunIntegrator: Send + Sync {
    fn run(&self, plan: &CodeIrProgram) -> RunResult;

    fn run_op(&self, op: &ExecutionOp, context: &ExecutionContext) -> RunResult {
        let _ = context;
        RunResult {
            success: false,
            failure_type: Some(
                execution_stability_core::failure::failure_type::FailureType::SandboxViolation,
            ),
            stdout: String::new(),
            stderr: format!("unsupported typed execution op: {}", op.label()),
            steps: vec![StepInfo {
                phase: op.label(),
                success: false,
                stdout: String::new(),
                stderr: "typed execution op unsupported by this integrator".to_string(),
            }],
        }
    }
}

// ── Hardened adapter ─────────────────────────────────────────────────────────

/// Adapts `HardenedExecutionController` to the `RunIntegrator` trait.
pub struct HardenedRunIntegrator {
    pub controller: execution_stability_core::hardening::HardenedExecutionController,
}

impl HardenedRunIntegrator {
    pub fn new() -> Self {
        Self {
            controller: execution_stability_core::hardening::HardenedExecutionController::default(),
        }
    }
}

impl Default for HardenedRunIntegrator {
    fn default() -> Self {
        Self::new()
    }
}

impl RunIntegrator for HardenedRunIntegrator {
    fn run(&self, plan: &CodeIrProgram) -> RunResult {
        use execution_stability_core::failure::failure_type::FailureType;

        match self.controller.execute_with_hardening(plan) {
            Ok(result) => {
                let steps = result
                    .base
                    .trace
                    .steps
                    .iter()
                    .map(|s| StepInfo {
                        phase: s.step_name.clone(),
                        success: s.success,
                        stdout: s.stdout.clone(),
                        stderr: s.stderr.clone(),
                    })
                    .collect();
                RunResult {
                    success: result.base.success,
                    failure_type: result.base.failure_type,
                    stdout: result.base.run_result.stdout.clone(),
                    stderr: result.base.run_result.stderr.clone(),
                    steps,
                }
            }
            Err(e) => {
                let ft = FailureType::from(e);
                RunResult {
                    success: false,
                    failure_type: Some(ft),
                    stdout: String::new(),
                    stderr: "Hardening layer error".to_string(),
                    steps: vec![],
                }
            }
        }
    }

    fn run_op(&self, op: &ExecutionOp, context: &ExecutionContext) -> RunResult {
        run_typed_git_op(op, &context.repo_root)
    }
}

// ── DryRunIntegrator (for tests) ──────────────────────────────────────────────

/// A `RunIntegrator` that always succeeds without executing anything.
/// Used for determinism tests and unit testing the strategy logic.
#[derive(Debug, Default)]
pub struct DryRunIntegrator;

impl RunIntegrator for DryRunIntegrator {
    fn run(&self, _plan: &CodeIrProgram) -> RunResult {
        RunResult {
            success: true,
            failure_type: None,
            stdout: "dry-run".to_string(),
            stderr: String::new(),
            steps: vec![StepInfo {
                phase: "dry-run".to_string(),
                success: true,
                stdout: "dry-run".to_string(),
                stderr: String::new(),
            }],
        }
    }

    fn run_op(&self, op: &ExecutionOp, _context: &ExecutionContext) -> RunResult {
        RunResult {
            success: true,
            failure_type: None,
            stdout: format!("dry-run {}", op.label()),
            stderr: String::new(),
            steps: vec![StepInfo {
                phase: op.label(),
                success: true,
                stdout: format!("dry-run {}", op.label()),
                stderr: String::new(),
            }],
        }
    }
}

/// A `RunIntegrator` that fails exactly `fail_count` times, then succeeds.
#[derive(Debug)]
pub struct FailThenSucceedIntegrator {
    fail_count: std::sync::atomic::AtomicU32,
    target: u32,
}

impl FailThenSucceedIntegrator {
    pub fn new(fail_count: u32) -> Self {
        Self {
            fail_count: std::sync::atomic::AtomicU32::new(0),
            target: fail_count,
        }
    }
}

impl RunIntegrator for FailThenSucceedIntegrator {
    fn run(&self, _plan: &CodeIrProgram) -> RunResult {
        use execution_stability_core::failure::failure_type::FailureType;
        let count = self
            .fail_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count < self.target {
            RunResult {
                success: false,
                failure_type: Some(FailureType::BuildFailure),
                stdout: String::new(),
                stderr: format!("Simulated failure #{}", count + 1),
                steps: vec![],
            }
        } else {
            RunResult {
                success: true,
                failure_type: None,
                stdout: "ok".to_string(),
                stderr: String::new(),
                steps: vec![],
            }
        }
    }
}

fn run_typed_git_op(op: &ExecutionOp, repo_root: &Path) -> RunResult {
    let label = op.label();
    let args = match git_args(op) {
        Ok(args) => args,
        Err(err) => return failed_git_result(label, err),
    };

    let command = match SandboxedCommand::new("git", repo_root).and_then(|cmd| cmd.args(args)) {
        Ok(command) => command,
        Err(err) => return failed_git_result(label, err.to_string()),
    };

    match command.run() {
        Ok(output) => {
            let stdout = format_git_output(op, &output.stdout);
            RunResult {
                success: output.success,
                failure_type: if output.success {
                    None
                } else {
                    Some(execution_stability_core::failure::failure_type::FailureType::RuntimeFailure)
                },
                stdout: stdout.clone(),
                stderr: String::new(),
                steps: vec![StepInfo {
                    phase: label,
                    success: output.success,
                    stdout,
                    stderr: format!("exit code: {:?}", output.exit_code),
                }],
            }
        }
        Err(err) => failed_git_result(label, err.to_string()),
    }
}

fn git_args(op: &ExecutionOp) -> Result<Vec<String>, String> {
    match op {
        ExecutionOp::GitStatus => Ok(vec!["status".to_string(), "--short".to_string()]),
        ExecutionOp::GitDiff => Ok(vec!["diff".to_string()]),
        ExecutionOp::GitAdd { path } => {
            validate_single_file(path)?;
            Ok(vec!["add".to_string(), path.clone()])
        }
        ExecutionOp::GitCommit { message } => {
            validate_commit_message(message)?;
            Ok(vec![
                "commit".to_string(),
                "-m".to_string(),
                message.clone(),
            ])
        }
        ExecutionOp::RuntimePhase(_) => {
            Err("generic external commands are not valid typed git operations".to_string())
        }
    }
}

fn validate_single_file(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path == "." {
        return Err("git add requires one explicit file path".to_string());
    }
    if path.contains('*') || path.contains('?') || path.contains('[') || path.contains(']') {
        return Err("git add rejects glob patterns".to_string());
    }
    let p = Path::new(path);
    if p.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("git add path must stay inside the fixed repository".to_string());
    }
    Ok(())
}

fn validate_commit_message(message: &str) -> Result<(), String> {
    if message == FIXED_GIT_COMMIT_MESSAGE {
        Ok(())
    } else {
        Err(format!(
            "git commit message is fixed to '{FIXED_GIT_COMMIT_MESSAGE}'"
        ))
    }
}

fn format_git_output(op: &ExecutionOp, stdout: &str) -> String {
    match op {
        ExecutionOp::GitStatus => format!(
            "[GIT STATUS]\n{}\n[NEXT]\n- git add <file>\n- git commit",
            stdout.trim()
        ),
        ExecutionOp::GitDiff => format!(
            "[GIT DIFF]\n{}\n[NEXT]\n- git add <file>\n- git commit",
            stdout.trim()
        ),
        ExecutionOp::GitAdd { path } => format!("[GIT ADD]\nstaged: {path}\n[NEXT]\n- git commit"),
        ExecutionOp::GitCommit { .. } => format!(
            "[GIT COMMIT]\n{}\n[STATE]\nrollback_available: false",
            stdout.trim()
        ),
        ExecutionOp::RuntimePhase(_) => stdout.to_string(),
    }
}

fn failed_git_result(phase: String, stderr: String) -> RunResult {
    RunResult {
        success: false,
        failure_type: Some(
            execution_stability_core::failure::failure_type::FailureType::SandboxViolation,
        ),
        stdout: String::new(),
        stderr: stderr.clone(),
        steps: vec![StepInfo {
            phase,
            success: false,
            stdout: String::new(),
            stderr,
        }],
    }
}

#[cfg(test)]
mod git_op_tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn git_add_rejects_dot() {
        let err = git_args(&ExecutionOp::GitAdd {
            path: ".".to_string(),
        })
        .unwrap_err();
        assert!(err.contains("explicit file"));
    }

    #[test]
    fn git_push_cannot_be_represented_as_typed_op() {
        let op = ExecutionOp::RuntimePhase("git push".to_string());
        let err = git_args(&op).unwrap_err();
        assert!(err.contains("generic external commands"));
    }

    #[test]
    fn git_commit_message_is_fixed() {
        let err = git_args(&ExecutionOp::GitCommit {
            message: "custom".to_string(),
        })
        .unwrap_err();
        assert!(err.contains(FIXED_GIT_COMMIT_MESSAGE));
        assert!(
            git_args(&ExecutionOp::GitCommit {
                message: FIXED_GIT_COMMIT_MESSAGE.to_string()
            })
            .is_ok()
        );
    }

    #[test]
    fn typed_git_status_executes_through_integrator() {
        let repo = temp_git_repo("status");
        let runner = HardenedRunIntegrator::default();
        let result = runner.run_op(
            &ExecutionOp::GitStatus,
            &ExecutionContext {
                repo_root: repo,
                ..ExecutionContext::default()
            },
        );
        assert!(result.success, "{}", result.stderr);
        assert!(result.stdout.contains("[GIT STATUS]"));
    }

    #[test]
    fn typed_git_commit_succeeds_with_fixed_message() {
        let repo = temp_git_repo("commit");
        fs::write(repo.join("src/lib.rs"), "pub fn changed() {}\n").expect("modify");
        let runner = HardenedRunIntegrator::default();
        let add = runner.run_op(
            &ExecutionOp::GitAdd {
                path: "src/lib.rs".to_string(),
            },
            &ExecutionContext {
                repo_root: repo.clone(),
                ..ExecutionContext::default()
            },
        );
        assert!(add.success, "{}", add.stderr);
        let commit = runner.run_op(
            &ExecutionOp::GitCommit {
                message: FIXED_GIT_COMMIT_MESSAGE.to_string(),
            },
            &ExecutionContext {
                repo_root: repo,
                ..ExecutionContext::default()
            },
        );
        assert!(commit.success, "{}", commit.stderr);
        assert!(commit.stdout.contains("[GIT COMMIT]"));
        assert!(commit.stdout.contains("rollback_available: false"));
    }

    fn temp_git_repo(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dbm_typed_git_{name}_{unique}"));
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(root.join("src/lib.rs"), "pub fn initial() {}\n").expect("lib");
        run_git(&root, &["init"]);
        run_git(&root, &["config", "user.email", "dbm@example.com"]);
        run_git(&root, &["config", "user.name", "DBM"]);
        run_git(&root, &["add", "src/lib.rs"]);
        run_git(&root, &["commit", "-m", "initial"]);
        root
    }

    fn run_git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
