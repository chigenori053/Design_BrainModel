use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::execution_foundation::{
    CommandSet, ExecAction, ExecReport, ExecutionFoundation, ProjectType,
};
use crate::runner::{
    ExecutionConfig, OutputMode, SandboxInstance, SandboxMode, SandboxPolicy, TimeoutConfig,
    create_sandbox, fixed_env, resolve_command, run as run_command,
};

#[path = "autonomous_execute_diagnostics.rs"]
mod diagnostics;
#[path = "autonomous_execute_git.rs"]
mod git_integration;
#[path = "autonomous_execute_remote.rs"]
mod remote_integration;

use diagnostics::{
    DebugEngine, FixGenerator, apply_text_patch, classify_error_line, default_tasks,
    execute_with_incident_recorder, extract_error_lines, has_progressed, matches_any,
    normalize_fix_chain_step, push_command, split_command,
};
use git_integration::{
    CommitDescriptor, GitExecutor, GitIntegration, PreCommitValidator, commit_message, diff_stats,
    finalize_git_integration, git_command, is_auto_fix_branch,
};
pub use git_integration::{DiffStats, GitAction, GitIntegrationReport};
use remote_integration::{
    AuthValidator, BranchManager, RemoteGuard, RemoteIntegration, finalize_remote_integration,
};
pub use remote_integration::{RemoteAction, RemoteIntegrationReport};

pub const MAX_RETRY: usize = 3;
pub const CONFIDENCE_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaskPlan {
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorCandidate {
    pub signature: String,
    pub signature_hint: String,
    pub action: String,
    pub priority: i32,
    pub confidence: f32,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<FixHint>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DebugResult {
    pub primary: ErrorCandidate,
    pub secondary: Vec<ErrorCandidate>,
    pub confidence: f32,
    pub retryable: bool,
    pub context_adjusted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextAttempt {
    pub signature: String,
    pub action: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ContextState {
    pub attempts: Vec<ContextAttempt>,
    pub seen_signatures: Vec<String>,
    pub applied_fixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ExecutionMetrics {
    pub attempts: usize,
    pub success: bool,
    pub fix_chain: Vec<String>,
    pub commit: bool,
    pub success_rate: u32,
    pub avg_retry_count: usize,
    pub failure_reason_distribution: BTreeMap<String, usize>,
}

pub struct ContextManager;

impl ContextManager {
    pub fn record_failure(context: &mut ContextState, candidate: &ErrorCandidate) {
        context.attempts.push(ContextAttempt {
            signature: candidate.signature.clone(),
            action: candidate.action.clone(),
            success: false,
        });
        context.seen_signatures.push(candidate.signature.clone());
    }

    pub fn record_success(
        context: &mut ContextState,
        signature: Option<&str>,
        action: Option<&str>,
    ) {
        if let (Some(signature), Some(action)) = (signature, action) {
            context.attempts.push(ContextAttempt {
                signature: signature.to_string(),
                action: action.to_string(),
                success: true,
            });
        }
    }

    pub fn record_fix(context: &mut ContextState, fix: &Fix) {
        context.applied_fixes.push(fix.content.clone());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteAttempt {
    pub attempt: usize,
    pub task: String,
    pub exec_report: ExecReport,
    pub debug: Option<DebugResult>,
    pub fix: Option<Fix>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutonomousExecuteReport {
    pub input: String,
    pub root: String,
    pub project_type: ProjectType,
    pub tasks: Vec<String>,
    pub attempts: Vec<ExecuteAttempt>,
    pub error_history: Vec<String>,
    pub context: ContextState,
    pub metrics: ExecutionMetrics,
    pub completed: bool,
    pub status: String,
    pub reason: Option<String>,
    pub retry_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitIntegrationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteIntegrationReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Fix {
    pub r#type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<TextPatch>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TextPatch {
    pub path: String,
    pub find: String,
    pub replace: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FixHint {
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CommandType {
    SafeRead,
    SafeWrite,
    RemoteWrite,
    Dangerous,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitIntegrationOptions {
    pub auto_commit: bool,
    pub require_confirmation: bool,
    pub no_commit: bool,
    pub dry_run: bool,
    pub rollback_on_failure: bool,
    pub auto_remote: bool,
    pub enable_remote: bool,
}

impl Default for GitIntegrationOptions {
    fn default() -> Self {
        Self {
            auto_commit: false,
            require_confirmation: true,
            no_commit: false,
            dry_run: false,
            rollback_on_failure: false,
            auto_remote: false,
            enable_remote: false,
        }
    }
}

impl ExecutionMetrics {
    fn from_run(
        attempts: &[ExecuteAttempt],
        retry_count: usize,
        completed: bool,
        reason: Option<&str>,
        git: Option<&GitIntegrationReport>,
    ) -> Self {
        let mut failure_reason_distribution = BTreeMap::new();
        if let Some(reason) = reason {
            failure_reason_distribution.insert(reason.to_string(), 1);
        }
        Self {
            attempts: attempts.len(),
            success: completed,
            fix_chain: attempts
                .iter()
                .filter_map(|attempt| attempt.debug.as_ref())
                .map(|debug| normalize_fix_chain_step(&debug.primary.action))
                .collect(),
            commit: git.is_some_and(|report| report.committed),
            success_rate: if completed { 1 } else { 0 },
            avg_retry_count: retry_count,
            failure_reason_distribution,
        }
    }
}

pub trait ConfirmationHandler {
    fn confirm_commit(&mut self, diff: &str) -> Result<bool, String>;
    fn confirm_remote(&mut self, prompt: &str) -> Result<bool, String>;
}

pub struct StdioConfirmationHandler;

impl ConfirmationHandler for StdioConfirmationHandler {
    fn confirm_commit(&mut self, diff: &str) -> Result<bool, String> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "Git diff:\n{diff}").map_err(|err| err.to_string())?;
        write!(stdout, "Apply commit? (y/n) ").map_err(|err| err.to_string())?;
        stdout.flush().map_err(|err| err.to_string())?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|err| format!("failed to read confirmation: {err}"))?;
        let normalized = input.trim().to_ascii_lowercase();
        Ok(matches!(normalized.as_str(), "y" | "yes"))
    }

    fn confirm_remote(&mut self, prompt: &str) -> Result<bool, String> {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{prompt} ").map_err(|err| err.to_string())?;
        stdout.flush().map_err(|err| err.to_string())?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|err| format!("failed to read confirmation: {err}"))?;
        let normalized = input.trim().to_ascii_lowercase();
        Ok(matches!(normalized.as_str(), "y" | "yes"))
    }
}

pub struct TaskPlanner;

impl TaskPlanner {
    pub fn plan(input: &str, project_type: ProjectType) -> TaskPlan {
        let normalized = input.to_lowercase();
        let commands = ExecutionFoundation::get_commands(project_type);
        let mut tasks = Vec::new();

        if matches_any(
            &normalized,
            &["install", "セットアップ", "依存", "dependency"],
        ) {
            push_command(&mut tasks, commands.install.as_ref());
        }
        if matches_any(&normalized, &["build", "ビルド", "compile", "コンパイル"]) {
            push_command(&mut tasks, commands.build.as_ref());
        }
        if matches_any(&normalized, &["test", "テスト"]) {
            push_command(&mut tasks, commands.test.as_ref());
        }
        if matches_any(&normalized, &["run", "実行", "起動"]) {
            push_command(&mut tasks, commands.run.as_ref());
        }

        if tasks.is_empty() {
            push_command(&mut tasks, commands.build.as_ref());
            if normalized.contains("test") || normalized.contains("テスト") {
                push_command(&mut tasks, commands.test.as_ref());
            }
        }

        if tasks.is_empty() {
            push_command(
                &mut tasks,
                default_tasks(&commands).first().map(|command| *command),
            );
        }

        TaskPlan { tasks }
    }
}

pub trait ExecuteAdapter {
    fn root(&self) -> &Path;
    fn sandbox_root(&self) -> &Path;
    fn project_type(&self) -> ProjectType;
    fn execute(&mut self, task: &str, timeout_ms: u64) -> Result<ExecReport, String>;
    fn apply_fix(&mut self, fix: &Fix, timeout_ms: u64) -> Result<(), String>;
}

pub struct IncidentExecutionAdapter {
    original_root: PathBuf,
    sandbox: SandboxInstance,
    project_type: ProjectType,
}

impl IncidentExecutionAdapter {
    pub fn new(path: &Path) -> Result<Self, String> {
        let original_root = path
            .canonicalize()
            .map_err(|err| format!("failed to resolve project path {}: {err}", path.display()))?;
        let sandbox = create_sandbox(&original_root).map_err(|err| err.to_string())?;
        let project_type = ExecutionFoundation::detect(sandbox.guard.path());
        Ok(Self {
            original_root,
            sandbox,
            project_type,
        })
    }
}

impl ExecuteAdapter for IncidentExecutionAdapter {
    fn root(&self) -> &Path {
        &self.original_root
    }

    fn sandbox_root(&self) -> &Path {
        self.sandbox.guard.path()
    }

    fn project_type(&self) -> ProjectType {
        self.project_type
    }

    fn execute(&mut self, task: &str, timeout_ms: u64) -> Result<ExecReport, String> {
        let command = split_command(task)?;
        execute_with_incident_recorder(
            &self.original_root,
            self.sandbox.guard.path(),
            self.project_type,
            command,
            timeout_ms,
        )
    }

    fn apply_fix(&mut self, fix: &Fix, timeout_ms: u64) -> Result<(), String> {
        if let Some(patch) = &fix.patch {
            apply_text_patch(self.sandbox.guard.path(), patch)?;
            return Ok(());
        }

        if let Some(command) = &fix.executable {
            let report = execute_with_incident_recorder(
                &self.original_root,
                self.sandbox.guard.path(),
                self.project_type,
                command.clone(),
                timeout_ms,
            )?;
            if report.success {
                return Ok(());
            }
            return Err(format!(
                "fix command failed with {}: {}",
                report.error_type, report.stderr
            ));
        }

        Err("fix is advisory only and cannot be applied automatically".to_string())
    }
}

pub fn execute_autonomous_command(
    path: &Path,
    input: &str,
    timeout_ms: u64,
) -> Result<AutonomousExecuteReport, String> {
    execute_autonomous_command_with_options(
        path,
        input,
        timeout_ms,
        GitIntegrationOptions::default(),
    )
}

pub fn execute_autonomous_command_with_options(
    path: &Path,
    input: &str,
    timeout_ms: u64,
    git_options: GitIntegrationOptions,
) -> Result<AutonomousExecuteReport, String> {
    let mut adapter = IncidentExecutionAdapter::new(path)?;
    let mut confirmer = StdioConfirmationHandler;
    execute_with_adapter_and_confirmation(
        &mut adapter,
        input,
        timeout_ms,
        git_options,
        &mut confirmer,
    )
}

pub fn execute_with_adapter<A: ExecuteAdapter>(
    adapter: &mut A,
    input: &str,
    timeout_ms: u64,
) -> Result<AutonomousExecuteReport, String> {
    let mut confirmer = NoopConfirmationHandler;
    execute_with_adapter_and_confirmation(
        adapter,
        input,
        timeout_ms,
        GitIntegrationOptions::default(),
        &mut confirmer,
    )
}

fn execute_with_adapter_and_confirmation<A: ExecuteAdapter, C: ConfirmationHandler>(
    adapter: &mut A,
    input: &str,
    timeout_ms: u64,
    git_options: GitIntegrationOptions,
    confirmer: &mut C,
) -> Result<AutonomousExecuteReport, String> {
    let plan = TaskPlanner::plan(input, adapter.project_type());
    let mut attempts: Vec<ExecuteAttempt> = Vec::new();
    let mut error_history = Vec::new();
    let mut context = ContextState::default();
    let mut retry_count = 0;

    for attempt in 1..=MAX_RETRY {
        let mut all_succeeded = true;
        for task in &plan.tasks {
            let report = adapter.execute(task, timeout_ms)?;
            if report.success {
                let previous = attempts.last().and_then(|attempt| attempt.debug.as_ref());
                ContextManager::record_success(
                    &mut context,
                    previous.map(|debug| debug.primary.signature.as_str()),
                    previous.map(|debug| debug.primary.action.as_str()),
                );
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report,
                    debug: None,
                    fix: None,
                    stop_reason: None,
                });
                continue;
            }

            all_succeeded = false;
            let debug = DebugEngine::analyze(&report, &context);
            let primary = &debug.primary;
            let prior_occurrences = context
                .seen_signatures
                .iter()
                .filter(|signature| *signature == &primary.signature)
                .count();

            if prior_occurrences >= 2 {
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report.clone(),
                    debug: Some(debug),
                    fix: None,
                    stop_reason: Some("signature_repeated".to_string()),
                });
                return Ok(build_report(
                    adapter,
                    input,
                    &plan,
                    attempts,
                    error_history,
                    context,
                    false,
                    Some(report.error_type),
                    retry_count,
                    None,
                    None,
                ));
            }

            ContextManager::record_failure(&mut context, primary);
            error_history.push(primary.signature.clone());

            if debug.confidence < CONFIDENCE_THRESHOLD {
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report.clone(),
                    debug: Some(debug),
                    fix: None,
                    stop_reason: Some("low_confidence".to_string()),
                });
                return Ok(build_report(
                    adapter,
                    input,
                    &plan,
                    attempts,
                    error_history,
                    context,
                    false,
                    Some(report.error_type),
                    retry_count,
                    None,
                    None,
                ));
            }

            if !debug.retryable {
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report.clone(),
                    debug: Some(debug),
                    fix: None,
                    stop_reason: Some("not_retryable".to_string()),
                });
                return Ok(build_report(
                    adapter,
                    input,
                    &plan,
                    attempts,
                    error_history,
                    context,
                    false,
                    Some(report.error_type),
                    retry_count,
                    None,
                    None,
                ));
            }

            if !has_progressed(&attempts, &debug) {
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report.clone(),
                    debug: Some(debug),
                    fix: None,
                    stop_reason: Some("no_progress".to_string()),
                });
                return Ok(build_report(
                    adapter,
                    input,
                    &plan,
                    attempts,
                    error_history,
                    context,
                    false,
                    Some(report.error_type),
                    retry_count,
                    None,
                    None,
                ));
            }

            let Some(fix) =
                FixGenerator::generate(primary, &report, adapter.project_type(), &context)
            else {
                attempts.push(ExecuteAttempt {
                    attempt,
                    task: task.clone(),
                    exec_report: report.clone(),
                    debug: Some(debug),
                    fix: None,
                    stop_reason: Some("no_fix".to_string()),
                });
                return Ok(build_report(
                    adapter,
                    input,
                    &plan,
                    attempts,
                    error_history,
                    context,
                    false,
                    Some(report.error_type),
                    retry_count,
                    None,
                    None,
                ));
            };

            match adapter.apply_fix(&fix, timeout_ms) {
                Ok(()) => {
                    retry_count += 1;
                    ContextManager::record_fix(&mut context, &fix);
                    attempts.push(ExecuteAttempt {
                        attempt,
                        task: task.clone(),
                        exec_report: report,
                        debug: Some(debug),
                        fix: Some(fix),
                        stop_reason: None,
                    });
                    break;
                }
                Err(_) => {
                    attempts.push(ExecuteAttempt {
                        attempt,
                        task: task.clone(),
                        exec_report: report.clone(),
                        debug: Some(debug),
                        fix: Some(fix),
                        stop_reason: Some("fix_apply_failed".to_string()),
                    });
                    return Ok(build_report(
                        adapter,
                        input,
                        &plan,
                        attempts,
                        error_history,
                        context,
                        false,
                        Some(report.error_type),
                        retry_count,
                        None,
                        None,
                    ));
                }
            }
        }

        if all_succeeded {
            let git = finalize_git_integration(
                adapter,
                &attempts,
                &plan.tasks,
                timeout_ms,
                git_options.clone(),
                confirmer,
            )?;
            let remote = finalize_remote_integration(
                adapter.root(),
                &attempts,
                git.as_ref(),
                &git_options,
                confirmer,
            )?;
            return Ok(build_report(
                adapter,
                input,
                &plan,
                attempts,
                error_history,
                context,
                true,
                None,
                retry_count,
                git,
                remote,
            ));
        }
    }

    let reason = attempts
        .last()
        .map(|attempt| attempt.exec_report.error_type.clone())
        .or_else(|| Some("Unknown".to_string()));
    Ok(build_report(
        adapter,
        input,
        &plan,
        attempts,
        error_history,
        context,
        false,
        reason,
        retry_count,
        None,
        None,
    ))
}

fn build_report<A: ExecuteAdapter>(
    adapter: &A,
    input: &str,
    plan: &TaskPlan,
    attempts: Vec<ExecuteAttempt>,
    error_history: Vec<String>,
    context: ContextState,
    completed: bool,
    reason: Option<String>,
    retry_count: usize,
    git: Option<GitIntegrationReport>,
    remote: Option<RemoteIntegrationReport>,
) -> AutonomousExecuteReport {
    let metrics = ExecutionMetrics::from_run(
        &attempts,
        retry_count,
        completed,
        reason.as_deref(),
        git.as_ref(),
    );
    AutonomousExecuteReport {
        input: input.to_string(),
        root: adapter.root().display().to_string(),
        project_type: adapter.project_type(),
        tasks: plan.tasks.clone(),
        attempts,
        error_history,
        context,
        metrics,
        completed,
        status: if completed {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        reason,
        retry_count,
        git,
        remote,
    }
}

struct NoopConfirmationHandler;

impl ConfirmationHandler for NoopConfirmationHandler {
    fn confirm_commit(&mut self, _diff: &str) -> Result<bool, String> {
        Ok(false)
    }

    fn confirm_remote(&mut self, _prompt: &str) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::render_autonomous_execute_report;
    use crate::runner::{OutputMeta, SandboxMode};
    use std::sync::{Mutex, OnceLock};

    #[derive(Debug, Clone)]
    struct MockAdapter {
        root: PathBuf,
        project_type: ProjectType,
        results: Vec<ExecReport>,
        applied_fixes: Vec<Fix>,
    }

    impl MockAdapter {
        fn new(project_type: ProjectType, results: Vec<ExecReport>) -> Self {
            Self {
                root: std::env::temp_dir(),
                project_type,
                results,
                applied_fixes: Vec::new(),
            }
        }
    }

    #[derive(Debug)]
    struct StaticConfirmationHandler(bool);

    impl ConfirmationHandler for StaticConfirmationHandler {
        fn confirm_commit(&mut self, _diff: &str) -> Result<bool, String> {
            Ok(self.0)
        }

        fn confirm_remote(&mut self, _prompt: &str) -> Result<bool, String> {
            Ok(self.0)
        }
    }

    fn temp_git_repo(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dbm_autonomous_git_{name}_{unique}"));
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write cargo");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");

        let init = git_command()
            .args(["init"])
            .current_dir(&root)
            .output()
            .expect("git init");
        assert!(init.status.success(), "git init failed");

        for (key, value) in [("user.email", "dbm@example.com"), ("user.name", "DBM")] {
            let config = git_command()
                .args(["config", key, value])
                .current_dir(&root)
                .output()
                .expect("git config");
            assert!(config.status.success(), "git config failed");
        }

        let add = git_command()
            .args(["add", "."])
            .current_dir(&root)
            .output()
            .expect("git add");
        assert!(add.status.success(), "git add failed");

        let commit = git_command()
            .args(["commit", "-m", "initial"])
            .current_dir(&root)
            .output()
            .expect("git commit");
        assert!(commit.status.success(), "git commit failed");

        let branch = git_command()
            .args(["checkout", "-b", "feature/test"])
            .current_dir(&root)
            .output()
            .expect("git checkout");
        assert!(branch.status.success(), "git checkout failed");
        root
    }

    fn gh_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var(self.key);
            }
        }
    }

    fn attach_origin_remote(repo: &Path, name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let bare = std::env::temp_dir().join(format!("dbm_autonomous_remote_{name}_{unique}.git"));
        let init = git_command()
            .args(["init", "--bare", bare.to_str().expect("utf8 bare path")])
            .output()
            .expect("git init bare");
        assert!(init.status.success(), "git init bare failed");

        let remote = git_command()
            .args([
                "remote",
                "add",
                "origin",
                bare.to_str().expect("utf8 bare path"),
            ])
            .current_dir(repo)
            .output()
            .expect("git remote add");
        assert!(remote.status.success(), "git remote add failed");
        bare
    }

    fn write_fake_gh(status_ok: bool) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let script = std::env::temp_dir().join(format!("dbm_fake_gh_{unique}.sh"));
        let body = if status_ok {
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  exit 0\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  echo \"https://github.com/example/repo/pull/1\"\n  exit 0\nfi\nexit 1\n"
        } else {
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  echo \"auth failed\" >&2\n  exit 1\nfi\nexit 1\n"
        };
        fs::write(&script, body).expect("write fake gh");
        let chmod = Command::new("chmod")
            .args(["+x", script.to_str().expect("utf8 script path")])
            .output()
            .expect("chmod fake gh");
        assert!(chmod.status.success(), "chmod fake gh failed");
        script
    }

    impl ExecuteAdapter for MockAdapter {
        fn root(&self) -> &Path {
            &self.root
        }

        fn sandbox_root(&self) -> &Path {
            &self.root
        }

        fn project_type(&self) -> ProjectType {
            self.project_type
        }

        fn execute(&mut self, _task: &str, _timeout_ms: u64) -> Result<ExecReport, String> {
            if self.results.is_empty() {
                return Err("no more mocked results".to_string());
            }
            Ok(self.results.remove(0))
        }

        fn apply_fix(&mut self, fix: &Fix, _timeout_ms: u64) -> Result<(), String> {
            self.applied_fixes.push(fix.clone());
            Ok(())
        }
    }

    fn report_for(
        project_type: ProjectType,
        success: bool,
        error_type: &str,
        command: &str,
        args: &[&str],
        stderr: &str,
        truncated: bool,
    ) -> ExecReport {
        ExecReport {
            root: ".".to_string(),
            project_type,
            action: if args.iter().any(|arg| *arg == "test") {
                ExecAction::Test
            } else {
                ExecAction::Build
            },
            status: if success {
                "success".to_string()
            } else if error_type == "Timeout" {
                "timeout".to_string()
            } else {
                "failure".to_string()
            },
            success,
            error_type: error_type.to_string(),
            exit_code: if success { 0 } else { 1 },
            duration_ms: 1,
            stdout: String::new(),
            stderr: stderr.to_string(),
            truncated,
            command: Some(command.to_string()),
            args: args.iter().map(|value| value.to_string()).collect(),
            output_meta: OutputMeta {
                streamed: false,
                truncated,
                original_size: 0,
            },
            stderr_meta: OutputMeta {
                streamed: false,
                truncated,
                original_size: stderr.len(),
            },
            sandbox_mode: Some(SandboxMode::Reuse),
            deterministic: true,
        }
    }

    fn report(
        success: bool,
        error_type: &str,
        command: &str,
        args: &[&str],
        stderr: &str,
        truncated: bool,
    ) -> ExecReport {
        report_for(
            ProjectType::Rust,
            success,
            error_type,
            command,
            args,
            stderr,
            truncated,
        )
    }

    #[test]
    fn planner_uses_template_based_commands() {
        let plan = TaskPlanner::plan("Rustプロジェクトをビルドしてテストして", ProjectType::Rust);
        assert_eq!(plan.tasks, vec!["cargo build", "cargo test"]);
    }

    #[test]
    fn extracts_multiple_rust_errors() {
        let errors = extract_error_lines(
            ProjectType::Rust,
            "error[E0463]: cannot find crate `serde`\nerror[E0432]: unresolved import `serde::Serialize`\nhelp: a similar path exists: `serde::ser::Serialize`",
        );
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("cannot find crate"));
        assert!(errors[1].contains("unresolved import"));
    }

    #[test]
    fn higher_priority_dependency_becomes_primary() {
        let result = DebugEngine::analyze(
            &report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "error[E0432]: unresolved import `serde::Serialize`\nerror[E0463]: cannot find crate `serde`",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(result.primary.action, "install_dependency");
        assert_eq!(result.primary.priority, 90);
        assert_eq!(result.secondary.len(), 1);
        assert_eq!(result.secondary[0].action, "add_use");
    }

    #[test]
    fn rust_stderr_pattern_maps_to_correct_action() {
        let result = DebugEngine::analyze(
            &report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "error[E0432]: unresolved import `crate::foo`\nhelp: a similar path exists: `crate::bar::foo`",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(result.primary.action, "add_use");
        assert_eq!(result.primary.signature_hint, "unresolved_import");
        assert!(result.primary.signature.starts_with("unresolved_import:"));
        assert!(result.primary.retryable);
    }

    #[test]
    fn node_stderr_pattern_maps_to_install_dependency() {
        let result = DebugEngine::analyze(
            &report_for(
                ProjectType::Node,
                false,
                "BuildError",
                "npm",
                &["run", "build"],
                "Module not found: Error: Can't resolve 'react-dom'",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(result.primary.action, "install_dependency");
        assert_eq!(result.primary.signature_hint, "module_not_found");
        assert!(result.primary.confidence >= 0.9);
    }

    #[test]
    fn rust_new_dictionary_matches_borrow_and_type_errors() {
        let borrow = DebugEngine::analyze(
            &report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "error[E0382]: borrow of moved value: `value`\n --> src/lib.rs:10:5",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(borrow.primary.action, "fix_borrow");
        let fix_type = DebugEngine::analyze(
            &report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "error[E0308]: mismatched types\nexpected struct `Foo`, found enum `Bar`",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(fix_type.primary.action, "fix_type");
    }

    #[test]
    fn node_new_dictionary_matches_reference_and_syntax_errors() {
        let reference = DebugEngine::analyze(
            &report_for(
                ProjectType::Node,
                false,
                "BuildError",
                "npm",
                &["run", "build"],
                "TypeError: undefined is not a function",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(reference.primary.action, "fix_reference");
        let syntax = DebugEngine::analyze(
            &report_for(
                ProjectType::Node,
                false,
                "BuildError",
                "npm",
                &["run", "build"],
                "SyntaxError: unexpected identifier",
                false,
            ),
            &ContextState::default(),
        );
        assert_eq!(syntax.primary.action, "fix_syntax");
    }

    #[test]
    fn confidence_low_aborts() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![report(
                false,
                "Unknown",
                "cargo",
                &["build"],
                "opaque failure",
                false,
            )],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(!run.completed);
        assert_eq!(
            run.attempts[0].stop_reason.as_deref(),
            Some("low_confidence")
        );
    }

    #[test]
    fn rust_dependency_error_generates_install_command() {
        let debug = DebugEngine::analyze(
            &report(
                false,
                "DependencyError",
                "cargo",
                &["build"],
                "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                false,
            ),
            &ContextState::default(),
        );
        let fix = FixGenerator::generate(
            &debug.primary,
            &report(
                false,
                "DependencyError",
                "cargo",
                &["build"],
                "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                false,
            ),
            ProjectType::Rust,
            &ContextState::default(),
        )
        .expect("dependency fix");
        assert_eq!(fix.content, "cargo add serde");
    }

    #[test]
    fn primary_only_fix_chains_to_success() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![
                report(
                    false,
                    "BuildError",
                    "cargo",
                    &["build"],
                    "error[E0463]: cannot find crate `serde`\nerror[E0432]: unresolved import `serde::Serialize`",
                    false,
                ),
                report(
                    false,
                    "BuildError",
                    "cargo",
                    &["build"],
                    "error[E0432]: unresolved import `serde::Serialize`\n --> src/lib.rs:1:5\nhelp: a similar path exists: `serde::ser::Serialize`",
                    false,
                ),
                report(true, "Unknown", "cargo", &["build"], "", false),
            ],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(run.completed);
        assert_eq!(adapter.applied_fixes.len(), 2);
        assert_eq!(adapter.applied_fixes[0].content, "cargo add serde");
        assert_eq!(adapter.applied_fixes[1].r#type, "patch");
        assert!(
            run.attempts[0]
                .debug
                .as_ref()
                .is_some_and(|debug| !debug.secondary.is_empty())
        );
    }

    #[test]
    fn rust_dependency_add_success() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![
                report(
                    false,
                    "DependencyError",
                    "cargo",
                    &["build"],
                    "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                    false,
                ),
                report(true, "Unknown", "cargo", &["build"], "", false),
            ],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(run.completed);
        assert_eq!(run.retry_count, 1);
        assert_eq!(adapter.applied_fixes.len(), 1);
        assert_eq!(adapter.applied_fixes[0].content, "cargo add serde");
    }

    #[test]
    fn node_dependency_add_success() {
        let mut adapter = MockAdapter::new(
            ProjectType::Node,
            vec![
                report_for(
                    ProjectType::Node,
                    false,
                    "DependencyError",
                    "npm",
                    &["run", "build"],
                    "Error: Cannot find module 'vite'",
                    false,
                ),
                report_for(
                    ProjectType::Node,
                    true,
                    "Unknown",
                    "npm",
                    &["run", "build"],
                    "",
                    false,
                ),
            ],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(run.completed);
        assert_eq!(adapter.applied_fixes[0].content, "npm install vite");
    }

    #[test]
    fn same_error_signature_stops_retry() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![
                report(
                    false,
                    "DependencyError",
                    "cargo",
                    &["build"],
                    "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                    false,
                ),
                report(
                    false,
                    "DependencyError",
                    "cargo",
                    &["build"],
                    "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                    false,
                ),
            ],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(!run.completed);
        assert_eq!(run.error_history.len(), 2);
        assert!(
            run.error_history
                .iter()
                .all(|signature| signature.starts_with("unresolved_module:"))
        );
        assert_eq!(
            run.attempts
                .last()
                .and_then(|attempt| attempt.stop_reason.as_deref()),
            Some("no_progress")
        );
    }

    #[test]
    fn recurring_signature_lowers_confidence_with_context() {
        let base = report(
            false,
            "DependencyError",
            "cargo",
            &["build"],
            "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
            false,
        );
        let baseline = DebugEngine::analyze(&base, &ContextState::default());
        let mut context = ContextState::default();
        ContextManager::record_failure(
            &mut context,
            &ErrorCandidate {
                signature: baseline.primary.signature.clone(),
                signature_hint: baseline.primary.signature_hint.clone(),
                action: baseline.primary.action.clone(),
                priority: baseline.primary.priority,
                confidence: baseline.primary.confidence,
                retryable: baseline.primary.retryable,
                hint: None,
            },
        );
        let result = DebugEngine::analyze(&base, &context);
        assert!(result.context_adjusted);
        assert!(result.confidence < 0.92);
    }

    #[test]
    fn duplicate_fix_is_not_applied_twice() {
        let candidate = ErrorCandidate {
            signature: "unresolved_module:123".to_string(),
            signature_hint: "unresolved_module".to_string(),
            action: "install_dependency".to_string(),
            priority: 90,
            confidence: 0.92,
            retryable: true,
            hint: None,
        };
        let mut context = ContextState::default();
        context.applied_fixes.push("cargo add serde".to_string());
        let fix = FixGenerator::generate(
            &candidate,
            &report(
                false,
                "DependencyError",
                "cargo",
                &["build"],
                "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                false,
            ),
            ProjectType::Rust,
            &context,
        );
        assert!(fix.is_none());
    }

    #[test]
    fn repeated_same_action_becomes_not_retryable() {
        let mut context = ContextState::default();
        context.attempts.push(ContextAttempt {
            signature: "unresolved_module:123".to_string(),
            action: "install_dependency".to_string(),
            success: false,
        });
        context.attempts.push(ContextAttempt {
            signature: "cannot_find_crate:456".to_string(),
            action: "install_dependency".to_string(),
            success: false,
        });
        let result = DebugEngine::analyze(
            &report(
                false,
                "DependencyError",
                "cargo",
                &["build"],
                "error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde`",
                false,
            ),
            &context,
        );
        assert!(!result.retryable);
        assert!(result.context_adjusted);
    }

    #[test]
    fn timeout_aborts_without_retry() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![report(
                false,
                "Timeout",
                "cargo",
                &["test"],
                "process timed out",
                false,
            )],
        );
        let run = execute_with_adapter(&mut adapter, "テストして", 10).expect("run");
        assert!(!run.completed);
        assert_eq!(run.reason.as_deref(), Some("Timeout"));
        assert!(adapter.applied_fixes.is_empty());
    }

    #[test]
    fn truncated_flag_is_preserved() {
        let mut adapter = MockAdapter::new(
            ProjectType::Rust,
            vec![report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "compile failed",
                true,
            )],
        );
        let run = execute_with_adapter(&mut adapter, "ビルドして", 1_000).expect("run");
        assert!(run.attempts[0].exec_report.truncated);
    }

    #[test]
    fn git_command_classifier_matches_phase1_rules() {
        assert_eq!(GitExecutor::classify(&["status"]), CommandType::SafeRead);
        assert_eq!(GitExecutor::classify(&["diff"]), CommandType::SafeRead);
        assert_eq!(
            GitExecutor::classify(&["log", "--oneline"]),
            CommandType::SafeRead
        );
        assert_eq!(GitExecutor::classify(&["branch"]), CommandType::SafeRead);
        assert_eq!(
            GitExecutor::classify(&["add", "Cargo.toml"]),
            CommandType::SafeWrite
        );
        assert_eq!(
            GitExecutor::classify(&["commit", "-m", "auto fix(dependency): add serde dependency"]),
            CommandType::SafeWrite
        );
        assert_eq!(GitExecutor::classify(&["add", "."]), CommandType::Dangerous);
        assert_eq!(
            GitExecutor::classify(&["commit", "--amend"]),
            CommandType::Dangerous
        );
        assert_eq!(GitExecutor::classify(&["rebase"]), CommandType::Dangerous);
    }

    #[test]
    fn remote_guard_rejects_protected_push_and_dangerous_gh_commands() {
        assert_eq!(
            RemoteGuard::classify_git(&["push", "origin", "dbm/auto-fix/20260327-120000"]),
            CommandType::RemoteWrite
        );
        assert_eq!(
            RemoteGuard::classify_git(&["push", "origin", "main"]),
            CommandType::Dangerous
        );
        assert_eq!(
            RemoteGuard::classify_git(&["rebase"]),
            CommandType::Dangerous
        );
        assert_eq!(
            RemoteGuard::classify_gh(&["pr", "status"]),
            CommandType::SafeRead
        );
        assert_eq!(
            RemoteGuard::classify_gh(&["repo", "delete", "sample"]),
            CommandType::Dangerous
        );
        assert_eq!(
            RemoteGuard::classify_gh(&["api", "repos/test"]),
            CommandType::Dangerous
        );
    }

    #[test]
    fn branch_manager_creates_auto_fix_branch() {
        let repo = temp_git_repo("branch_manager");
        let branch = BranchManager::create(&repo).expect("create branch");
        assert!(is_auto_fix_branch(&branch));

        let current = GitExecutor::current_branch(&repo)
            .expect("current branch")
            .expect("branch value");
        assert_eq!(current, branch);
    }

    #[test]
    fn dangerous_git_add_dot_is_rejected() {
        let repo = temp_git_repo("dangerous_add");
        let error = GitExecutor::run_checked(&repo, &["add", "."]).expect_err("must reject");
        assert!(error.contains("dangerous git command rejected"));
    }

    #[test]
    fn commit_message_uses_auto_fix_template() {
        assert_eq!(
            commit_message(&CommitDescriptor {
                kind: "import",
                detail: "add trait import".to_string(),
            }),
            "auto fix(import): add trait import"
        );
    }

    #[test]
    fn git_status_safe_read_executes() {
        let repo = temp_git_repo("status_read");
        GitExecutor::run_checked(&repo, &["status"]).expect("git status");
    }

    #[test]
    fn pre_commit_validator_rejects_large_diff() {
        let diff = (0..205)
            .map(|index| format!("+line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let stats = diff_stats(&diff);
        let repo = temp_git_repo("large_diff");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(sandbox.join("src")).expect("create sandbox");
        fs::write(sandbox.join("src/main.rs"), "fn main() {}\n").expect("write sandbox file");
        let reason = PreCommitValidator::validate(&repo, "src/main.rs", &diff, &stats, &sandbox)
            .expect("validator");
        assert_eq!(reason.as_deref(), Some("diff_too_large"));
    }

    #[test]
    fn pre_commit_validator_rejects_blocked_file_types() {
        let repo = temp_git_repo("blocked_type");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(&sandbox).expect("create sandbox");
        fs::write(sandbox.join("Cargo.lock"), "version = 4\n").expect("write lock");
        let reason = PreCommitValidator::validate(
            &repo,
            "Cargo.lock",
            "--- Cargo.lock\n+version = 4\n",
            &DiffStats {
                lines_added: 1,
                lines_removed: 0,
            },
            &sandbox,
        )
        .expect("validator");
        assert_eq!(reason.as_deref(), Some("blocked_file_type"));
    }

    #[test]
    fn git_state_validation_detects_merge_in_progress() {
        let repo = temp_git_repo("merge_state");
        fs::write(repo.join(".git/MERGE_HEAD"), "deadbeef\n").expect("write merge head");
        let state = GitExecutor::validate_repository_state(&repo).expect("validate state");
        assert_eq!(state.as_deref(), Some("merge_in_progress"));
    }

    #[test]
    fn auth_validator_detects_gh_auth_failure() {
        let _guard = gh_env_lock().lock().expect("gh env lock");
        let repo = temp_git_repo("auth_failure");
        let _remote = attach_origin_remote(&repo, "auth_failure");
        let fake_gh = write_fake_gh(false);
        let _env = EnvVarGuard::set_path("DBM_GH_BIN", &fake_gh);

        let state = AuthValidator::validate(&repo).expect("auth validate");
        assert_eq!(state.as_deref(), Some("github_auth_invalid"));
    }

    #[test]
    fn remote_integration_pushes_and_creates_pr() {
        let _guard = gh_env_lock().lock().expect("gh env lock");
        let repo = temp_git_repo("remote_success");
        let bare = attach_origin_remote(&repo, "remote_success");
        let fake_gh = write_fake_gh(true);
        let _env = EnvVarGuard::set_path("DBM_GH_BIN", &fake_gh);

        let attempts = vec![ExecuteAttempt {
            attempt: 1,
            task: "cargo build".to_string(),
            exec_report: report(true, "None", "cargo", &["build"], "", false),
            debug: Some(DebugResult {
                primary: ErrorCandidate {
                    signature: "unresolved_import:123".to_string(),
                    signature_hint: "unresolved_import".to_string(),
                    action: "add_use".to_string(),
                    priority: 80,
                    confidence: 0.9,
                    retryable: true,
                    hint: None,
                },
                secondary: Vec::new(),
                confidence: 0.9,
                retryable: true,
                context_adjusted: false,
            }),
            fix: Some(Fix {
                r#type: "patch".to_string(),
                content: "use std::fmt::Display;".to_string(),
                executable: None,
                patch: None,
            }),
            stop_reason: None,
        }];
        let git = GitIntegrationReport {
            changed_files: vec!["src/main.rs".to_string()],
            diff: String::new(),
            diff_stats: DiffStats::default(),
            actions: Vec::new(),
            committed: true,
            confirmation_required: false,
            confirmation_granted: true,
            rolled_back: false,
            commit_id: Some("abc123".to_string()),
            reason: None,
        };

        let remote = RemoteIntegration::finalize(
            &repo,
            &attempts,
            Some(&git),
            &GitIntegrationOptions {
                auto_commit: true,
                require_confirmation: false,
                no_commit: false,
                dry_run: false,
                rollback_on_failure: false,
                auto_remote: true,
                enable_remote: true,
            },
            &mut StaticConfirmationHandler(true),
        )
        .expect("remote finalize")
        .expect("remote report");

        assert!(remote.pushed);
        assert!(remote.pr_created);
        assert!(remote.branch.as_deref().is_some_and(is_auto_fix_branch));
        assert_eq!(
            remote.pr_url.as_deref(),
            Some("https://github.com/example/repo/pull/1")
        );

        let heads = git_command()
            .args([
                "--git-dir",
                bare.to_str().expect("utf8 bare path"),
                "show-ref",
                "--heads",
            ])
            .output()
            .expect("git show-ref");
        let refs = String::from_utf8_lossy(&heads.stdout);
        assert!(
            remote
                .branch
                .as_deref()
                .is_some_and(|branch| refs.contains(branch))
        );
    }

    #[test]
    fn single_file_git_integration_can_commit_after_confirmation() {
        let repo = temp_git_repo("single_file_commit");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(sandbox.join("src")).expect("create sandbox src");
        fs::write(
            sandbox.join("Cargo.toml"),
            fs::read(repo.join("Cargo.toml")).expect("read cargo"),
        )
        .expect("copy cargo");
        fs::write(
            sandbox.join("src/main.rs"),
            "use std::fmt::Display;\nfn main() {}\n",
        )
        .expect("write sandbox source");

        let attempts = vec![ExecuteAttempt {
            attempt: 1,
            task: "cargo build".to_string(),
            exec_report: report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "unresolved import",
                false,
            ),
            debug: Some(DebugResult {
                primary: ErrorCandidate {
                    signature: "unresolved_import:123".to_string(),
                    signature_hint: "unresolved_import".to_string(),
                    action: "add_use".to_string(),
                    priority: 80,
                    confidence: 0.9,
                    retryable: true,
                    hint: None,
                },
                secondary: Vec::new(),
                confidence: 0.9,
                retryable: true,
                context_adjusted: false,
            }),
            fix: Some(Fix {
                r#type: "patch".to_string(),
                content: "use std::fmt::Display;".to_string(),
                executable: None,
                patch: Some(TextPatch {
                    path: "src/main.rs".to_string(),
                    find: String::new(),
                    replace: "use std::fmt::Display;\n".to_string(),
                }),
            }),
            stop_reason: None,
        }];

        let mut confirmer = StaticConfirmationHandler(true);
        let git = GitIntegration::finalize(
            &repo,
            &sandbox,
            &attempts,
            &["cargo build".to_string()],
            1_000,
            GitIntegrationOptions {
                auto_commit: false,
                require_confirmation: true,
                no_commit: false,
                dry_run: false,
                rollback_on_failure: false,
                auto_remote: false,
                enable_remote: false,
            },
            &mut confirmer,
        )
        .expect("git finalize")
        .expect("git report");

        assert!(git.committed);
        assert_eq!(git.changed_files, vec!["src/main.rs".to_string()]);
        assert!(git.commit_id.is_some());
        assert_eq!(
            fs::read_to_string(repo.join("src/main.rs")).expect("read updated source"),
            "use std::fmt::Display;\nfn main() {}\n"
        );
    }

    #[test]
    fn git_integration_respects_confirmation() {
        let repo = temp_git_repo("confirm_decline");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(sandbox.join("src")).expect("create sandbox src");
        fs::write(
            sandbox.join("Cargo.toml"),
            fs::read(repo.join("Cargo.toml")).expect("read cargo"),
        )
        .expect("copy cargo");
        fs::write(
            sandbox.join("src/main.rs"),
            "use std::fmt::Display;\nfn main() {}\n",
        )
        .expect("write sandbox source");

        let attempts = vec![ExecuteAttempt {
            attempt: 1,
            task: "cargo build".to_string(),
            exec_report: report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "unresolved import",
                false,
            ),
            debug: Some(DebugResult {
                primary: ErrorCandidate {
                    signature: "unresolved_import:123".to_string(),
                    signature_hint: "unresolved_import".to_string(),
                    action: "add_use".to_string(),
                    priority: 80,
                    confidence: 0.9,
                    retryable: true,
                    hint: None,
                },
                secondary: Vec::new(),
                confidence: 0.9,
                retryable: true,
                context_adjusted: false,
            }),
            fix: Some(Fix {
                r#type: "patch".to_string(),
                content: "use std::fmt::Display;".to_string(),
                executable: None,
                patch: Some(TextPatch {
                    path: "src/main.rs".to_string(),
                    find: String::new(),
                    replace: "use std::fmt::Display;\n".to_string(),
                }),
            }),
            stop_reason: None,
        }];

        let mut confirmer = StaticConfirmationHandler(false);
        let git = GitIntegration::finalize(
            &repo,
            &sandbox,
            &attempts,
            &["cargo build".to_string()],
            1_000,
            GitIntegrationOptions {
                auto_commit: false,
                require_confirmation: true,
                no_commit: false,
                dry_run: false,
                rollback_on_failure: false,
                auto_remote: false,
                enable_remote: false,
            },
            &mut confirmer,
        )
        .expect("git finalize")
        .expect("git report");

        assert!(!git.committed);
        assert_eq!(git.reason.as_deref(), Some("commit_declined"));
        let head = Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&repo)
            .output()
            .expect("git log");
        let log = String::from_utf8_lossy(&head.stdout);
        assert!(log.contains("initial"));
    }

    #[test]
    fn git_integration_rejects_multi_file_changes() {
        let repo = temp_git_repo("multi_file_reject");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(sandbox.join("src")).expect("create sandbox src");
        fs::write(
            sandbox.join("Cargo.toml"),
            "[package]\nname = \"sample\"\nversion = \"0.1.1\"\nedition = \"2024\"\n",
        )
        .expect("write sandbox cargo");
        fs::write(
            sandbox.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .expect("write sandbox source");

        let attempts = vec![ExecuteAttempt {
            attempt: 1,
            task: "cargo build".to_string(),
            exec_report: report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "compile failed",
                false,
            ),
            debug: None,
            fix: Some(Fix {
                r#type: "patch".to_string(),
                content: "multi".to_string(),
                executable: None,
                patch: Some(TextPatch {
                    path: "src/main.rs".to_string(),
                    find: String::new(),
                    replace: "fn main() { println!(\"hi\"); }\n".to_string(),
                }),
            }),
            stop_reason: None,
        }];

        let mut confirmer = StaticConfirmationHandler(true);
        let git = GitIntegration::finalize(
            &repo,
            &sandbox,
            &attempts,
            &["cargo build".to_string()],
            1_000,
            GitIntegrationOptions {
                auto_commit: true,
                require_confirmation: false,
                no_commit: false,
                dry_run: false,
                rollback_on_failure: false,
                auto_remote: false,
                enable_remote: false,
            },
            &mut confirmer,
        )
        .expect("git finalize")
        .expect("git report");

        assert!(!git.committed);
        assert_eq!(git.reason.as_deref(), Some("single_file_rule_violation"));
    }

    #[test]
    fn rollback_restores_original_file_after_post_commit_failure() {
        let repo = temp_git_repo("rollback_failure");
        let sandbox = repo.with_extension("sandbox");
        fs::create_dir_all(sandbox.join("src")).expect("create sandbox src");
        fs::write(
            sandbox.join("Cargo.toml"),
            fs::read(repo.join("Cargo.toml")).expect("read cargo"),
        )
        .expect("copy cargo");
        fs::write(
            sandbox.join("src/main.rs"),
            "fn main() { let _: i32 = \"broken\"; }\n",
        )
        .expect("write broken source");

        let attempts = vec![ExecuteAttempt {
            attempt: 1,
            task: "cargo build".to_string(),
            exec_report: report(
                false,
                "BuildError",
                "cargo",
                &["build"],
                "compile failed",
                false,
            ),
            debug: Some(DebugResult {
                primary: ErrorCandidate {
                    signature: "fix_compile:123".to_string(),
                    signature_hint: "build_error".to_string(),
                    action: "fix_compile".to_string(),
                    priority: 60,
                    confidence: 0.9,
                    retryable: true,
                    hint: Some(FixHint {
                        kind: "patch".to_string(),
                        payload: "src/main.rs|fn main() {}|fn main() { let _: i32 = \"broken\"; }"
                            .to_string(),
                    }),
                },
                secondary: Vec::new(),
                confidence: 0.9,
                retryable: true,
                context_adjusted: false,
            }),
            fix: Some(Fix {
                r#type: "patch".to_string(),
                content: "src/main.rs broken".to_string(),
                executable: None,
                patch: Some(TextPatch {
                    path: "src/main.rs".to_string(),
                    find: "fn main() {}\n".to_string(),
                    replace: "fn main() { let _: i32 = \"broken\"; }\n".to_string(),
                }),
            }),
            stop_reason: None,
        }];

        let mut confirmer = StaticConfirmationHandler(true);
        let git = GitIntegration::finalize(
            &repo,
            &sandbox,
            &attempts,
            &["cargo build".to_string()],
            1_000,
            GitIntegrationOptions {
                auto_commit: true,
                require_confirmation: false,
                no_commit: false,
                dry_run: false,
                rollback_on_failure: true,
                auto_remote: false,
                enable_remote: false,
            },
            &mut confirmer,
        )
        .expect("git finalize")
        .expect("git report");

        assert!(git.committed);
        assert!(git.rolled_back);
        assert!(
            git.reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("rolled_back_after_failure:"))
        );
        assert_eq!(
            fs::read_to_string(repo.join("src/main.rs")).expect("read restored source"),
            "fn main() {}\n"
        );
    }

    #[test]
    fn log_output_includes_error_action_fix_and_success() {
        let report = AutonomousExecuteReport {
            input: "ビルドして".to_string(),
            root: ".".to_string(),
            project_type: ProjectType::Rust,
            tasks: vec!["cargo build".to_string()],
            attempts: vec![
                ExecuteAttempt {
                    attempt: 1,
                    task: "cargo build".to_string(),
                    exec_report: report(
                        false,
                        "BuildError",
                        "cargo",
                        &["build"],
                        "failed to compile",
                        false,
                    ),
                    debug: Some(DebugResult {
                        primary: ErrorCandidate {
                            signature: "cannot_find_crate:abcdef".to_string(),
                            signature_hint: "cannot_find_crate".to_string(),
                            action: "install_dependency".to_string(),
                            priority: 90,
                            confidence: 0.9,
                            retryable: true,
                            hint: None,
                        },
                        secondary: Vec::new(),
                        confidence: 0.9,
                        retryable: true,
                        context_adjusted: false,
                    }),
                    fix: Some(Fix {
                        r#type: "command".to_string(),
                        content: "cargo add serde".to_string(),
                        executable: Some(vec![
                            "cargo".to_string(),
                            "add".to_string(),
                            "serde".to_string(),
                        ]),
                        patch: None,
                    }),
                    stop_reason: None,
                },
                ExecuteAttempt {
                    attempt: 2,
                    task: "cargo build".to_string(),
                    exec_report: report(true, "Unknown", "cargo", &["build"], "", false),
                    debug: None,
                    fix: None,
                    stop_reason: None,
                },
            ],
            error_history: vec!["cannot_find_crate:abcdef".to_string()],
            context: ContextState::default(),
            metrics: ExecutionMetrics {
                attempts: 2,
                success: true,
                fix_chain: vec!["dependency".to_string()],
                commit: false,
                success_rate: 1,
                avg_retry_count: 1,
                failure_reason_distribution: BTreeMap::new(),
            },
            completed: true,
            status: "success".to_string(),
            reason: None,
            retry_count: 1,
            git: None,
            remote: None,
        };
        let mut output = Vec::new();
        render_autonomous_execute_report(&mut output, &report).expect("render");
        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("Attempt 1:"));
        assert!(rendered.contains("Error: cannot_find_crate"));
        assert!(rendered.contains("Signature: cannot_find_crate:abcdef"));
        assert!(rendered.contains("Action: install_dependency"));
        assert!(rendered.contains("Confidence: 0.90"));
        assert!(rendered.contains("Context Adjusted: false"));
        assert!(rendered.contains("Fix: cargo add serde"));
        assert!(rendered.contains("Attempt 2:"));
        assert!(rendered.contains("Success"));
        assert!(rendered.contains("success_rate: 1"));
    }

    #[test]
    fn signature_normalization_reduces_duplicates() {
        let left = classify_error_line(
            ProjectType::Rust,
            "error[E0432]: unresolved import `serde::Serialize` --> src/lib.rs:12:4",
            None,
        );
        let right = classify_error_line(
            ProjectType::Rust,
            "error[E0432]: unresolved import `serde::Serialize` --> src/main.rs:44:9",
            None,
        );
        assert_eq!(left.signature, right.signature);
    }
}
