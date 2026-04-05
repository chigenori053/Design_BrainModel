use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitAction {
    pub r#type: String,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitIntegrationReport {
    pub changed_files: Vec<String>,
    pub diff: String,
    pub diff_stats: DiffStats,
    pub actions: Vec<GitAction>,
    pub committed: bool,
    pub confirmation_required: bool,
    pub confirmation_granted: bool,
    pub rolled_back: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct DiffStats {
    pub lines_added: usize,
    pub lines_removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommitDescriptor<'a> {
    pub kind: &'a str,
    pub detail: String,
}

pub(super) struct GitIntegration;

impl GitIntegration {
    pub(super) fn finalize<C: ConfirmationHandler>(
        root: &Path,
        sandbox_root: &Path,
        attempts: &[ExecuteAttempt],
        tasks: &[String],
        timeout_ms: u64,
        options: GitIntegrationOptions,
        confirmer: &mut C,
    ) -> Result<Option<GitIntegrationReport>, String> {
        if !attempts.iter().any(|attempt| attempt.fix.is_some()) {
            return Ok(None);
        }

        if !root.join(".git").exists() {
            return Ok(Some(GitIntegrationReport {
                changed_files: Vec::new(),
                diff: String::new(),
                diff_stats: DiffStats::default(),
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("git_repository_not_found".to_string()),
            }));
        }

        if let Some(reason) = GitExecutor::validate_repository_state(root)? {
            return Ok(Some(GitIntegrationReport {
                changed_files: Vec::new(),
                diff: String::new(),
                diff_stats: DiffStats::default(),
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some(reason),
            }));
        }

        let changed_files = collect_changed_files(root, sandbox_root)?;
        if changed_files.is_empty() {
            return Ok(Some(GitIntegrationReport {
                changed_files,
                diff: String::new(),
                diff_stats: DiffStats::default(),
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("no_local_changes".to_string()),
            }));
        }

        if changed_files.len() != 1 {
            return Ok(Some(GitIntegrationReport {
                changed_files,
                diff: String::new(),
                diff_stats: DiffStats::default(),
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("single_file_rule_violation".to_string()),
            }));
        }

        let file = changed_files[0].clone();
        if GitExecutor::has_local_changes(root, &file)? {
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff: String::new(),
                diff_stats: DiffStats::default(),
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("target_file_already_dirty".to_string()),
            }));
        }

        let original_bytes = fs::read(root.join(&file)).ok();
        let diff = if options.dry_run {
            diff_between_paths(&root.join(&file), &sandbox_root.join(&file))?
        } else {
            sync_changed_file(root, sandbox_root, &file)?;
            GitExecutor::diff(root, &file)?
        };
        let diff_stats = diff_stats(&diff);
        if let Some(reason) =
            PreCommitValidator::validate(root, &file, &diff, &diff_stats, sandbox_root)?
        {
            if !options.dry_run {
                restore_original_file(root, &file, original_bytes.as_deref())?;
            }
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff,
                diff_stats,
                actions: Vec::new(),
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some(reason),
            }));
        }

        let descriptor = commit_descriptor(attempts);
        let message = commit_message(&descriptor);
        let actions = vec![
            GitAction {
                r#type: "add".to_string(),
                files: vec![file.clone()],
                message: None,
            },
            GitAction {
                r#type: "commit".to_string(),
                files: vec![file.clone()],
                message: Some(message.clone()),
            },
        ];

        if options.dry_run {
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff,
                diff_stats,
                actions,
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("dry_run".to_string()),
            }));
        }

        if options.no_commit {
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff,
                diff_stats,
                actions,
                committed: false,
                confirmation_required: false,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some("no_commit".to_string()),
            }));
        }

        let confirmation_required = !options.auto_commit;
        let confirmation_granted = if options.auto_commit {
            true
        } else if options.require_confirmation {
            confirmer.confirm_commit(&diff)?
        } else {
            false
        };

        if !confirmation_granted {
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff,
                diff_stats,
                actions,
                committed: false,
                confirmation_required,
                confirmation_granted: false,
                rolled_back: false,
                commit_id: None,
                reason: Some(if options.require_confirmation {
                    "commit_declined".to_string()
                } else {
                    "confirmation_required".to_string()
                }),
            }));
        }

        let commit_id = GitExecutor::commit(root, &file, &message)?;
        if let Some(verification_reason) = verify_post_commit(root, tasks, timeout_ms)? {
            let rolled_back = if options.rollback_on_failure {
                GitExecutor::rollback_last_commit(root)?;
                restore_original_file(root, &file, original_bytes.as_deref())?;
                true
            } else {
                false
            };
            return Ok(Some(GitIntegrationReport {
                changed_files: vec![file],
                diff,
                diff_stats,
                actions,
                committed: true,
                confirmation_required,
                confirmation_granted: true,
                rolled_back,
                commit_id: Some(commit_id),
                reason: Some(if rolled_back {
                    format!("rolled_back_after_failure:{verification_reason}")
                } else {
                    format!("post_commit_verification_failed:{verification_reason}")
                }),
            }));
        }

        Ok(Some(GitIntegrationReport {
            changed_files: vec![file],
            diff,
            diff_stats,
            actions,
            committed: true,
            confirmation_required,
            confirmation_granted: true,
            rolled_back: false,
            commit_id: Some(commit_id),
            reason: None,
        }))
    }
}

pub(super) struct GitExecutor;

impl GitExecutor {
    pub(super) fn classify(args: &[&str]) -> CommandType {
        match args {
            ["status"] | ["diff"] | ["branch"] => CommandType::SafeRead,
            ["log", "--oneline"] => CommandType::SafeRead,
            ["checkout", "-b", branch] if is_auto_fix_branch(branch) => CommandType::SafeWrite,
            ["add", path] if is_explicit_single_file(path) => CommandType::SafeWrite,
            ["commit", "-m", message] if is_valid_commit_message(message) => CommandType::SafeWrite,
            ["add", _]
            | ["reset", ..]
            | ["commit", "--amend", ..]
            | ["commit", ..]
            | ["rebase", ..]
            | ["clean", ..] => CommandType::Dangerous,
            _ => CommandType::Dangerous,
        }
    }

    pub(super) fn diff(root: &Path, file: &str) -> Result<String, String> {
        let output = git_command()
            .args(["diff", "--", file])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git diff: {err}"))?;
        if output.status.success() {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if !diff.trim().is_empty() {
                return Ok(diff);
            }
        }

        let file_path = root.join(file);
        let output = git_command()
            .args(["diff", "--no-index", "--", "/dev/null"])
            .arg(&file_path)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run fallback git diff: {err}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub(super) fn commit(root: &Path, file: &str, message: &str) -> Result<String, String> {
        Self::run_checked(root, &["add", file])?;
        let args = ["commit", "-m", message];
        match Self::classify(&args) {
            CommandType::SafeWrite => {}
            _ => return Err("dangerous git commit rejected".to_string()),
        }
        let output = git_command()
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git commit: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Self::current_commit(root)?
            .ok_or_else(|| format!("commit succeeded but HEAD is unavailable for {file}"))
    }

    pub(super) fn run_checked(root: &Path, args: &[&str]) -> Result<(), String> {
        match Self::classify(args) {
            CommandType::SafeRead | CommandType::SafeWrite => {}
            CommandType::Dangerous => {
                return Err(format!(
                    "dangerous git command rejected: git {}",
                    args.join(" ")
                ));
            }
        }
        let output = git_command()
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git {}: {err}", args.join(" ")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    pub(super) fn current_commit(root: &Path) -> Result<Option<String>, String> {
        let output = git_command()
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to resolve commit id: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if commit.is_empty() {
            Ok(None)
        } else {
            Ok(Some(commit))
        }
    }

    pub(super) fn has_local_changes(root: &Path, file: &str) -> Result<bool, String> {
        let output = git_command()
            .args(["status", "--porcelain", "--", file])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to inspect git status: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    pub(super) fn validate_repository_state(root: &Path) -> Result<Option<String>, String> {
        if root.join(".git").join("MERGE_HEAD").exists() {
            return Ok(Some("merge_in_progress".to_string()));
        }
        if root.join(".git").join("rebase-merge").exists()
            || root.join(".git").join("rebase-apply").exists()
        {
            return Ok(Some("rebase_in_progress".to_string()));
        }

        let output = git_command()
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to inspect git status: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let status = String::from_utf8_lossy(&output.stdout);
        if status
            .lines()
            .any(|line| line.starts_with("UU") || line.starts_with("AA") || line.starts_with("DD"))
        {
            return Ok(Some("git_conflict_detected".to_string()));
        }
        if let Some(branch) = Self::current_branch(root)? {
            if is_protected_branch(&branch) {
                return Ok(Some("protected_branch_checked_out".to_string()));
            }
        }
        Ok(None)
    }

    pub(super) fn current_branch(root: &Path) -> Result<Option<String>, String> {
        let output = git_command()
            .args(["branch", "--show-current"])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to resolve current branch: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() {
            Ok(None)
        } else {
            Ok(Some(branch))
        }
    }

    pub(super) fn rollback_last_commit(root: &Path) -> Result<(), String> {
        let output = git_output_with_retry(root, ["reset", "HEAD~1"])
            .map_err(|err| format!("failed to rollback last commit: {err}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

pub(super) struct PreCommitValidator;

impl PreCommitValidator {
    pub(super) fn validate(
        root: &Path,
        file: &str,
        diff: &str,
        stats: &DiffStats,
        sandbox_root: &Path,
    ) -> Result<Option<String>, String> {
        if diff.lines().count() > 200 {
            return Ok(Some("diff_too_large".to_string()));
        }
        if !is_allowed_file_type(file) {
            return Ok(Some("blocked_file_type".to_string()));
        }
        let bytes = fs::read(sandbox_root.join(file))
            .or_else(|_| fs::read(root.join(file)))
            .map_err(|err| format!("failed to inspect changed file {file}: {err}"))?;
        if bytes.contains(&0) {
            return Ok(Some("binary_change_blocked".to_string()));
        }
        let _ = stats;
        Ok(None)
    }
}

pub(super) fn finalize_git_integration<A: ExecuteAdapter, C: ConfirmationHandler>(
    adapter: &A,
    attempts: &[ExecuteAttempt],
    tasks: &[String],
    timeout_ms: u64,
    options: GitIntegrationOptions,
    confirmer: &mut C,
) -> Result<Option<GitIntegrationReport>, String> {
    GitIntegration::finalize(
        adapter.root(),
        adapter.sandbox_root(),
        attempts,
        tasks,
        timeout_ms,
        options,
        confirmer,
    )
}

fn collect_changed_files(root: &Path, sandbox_root: &Path) -> Result<Vec<String>, String> {
    let mut paths = BTreeSet::new();
    collect_relative_files(root, root, &mut paths)?;
    collect_relative_files(sandbox_root, sandbox_root, &mut paths)?;

    let mut changed = Vec::new();
    for relative in paths {
        if relative.starts_with(".git/")
            || relative.starts_with("target/")
            || relative.starts_with("node_modules/")
            || relative.starts_with(".dbm_autonomous_execute/")
            || relative.starts_with("runtime/incidents/")
        {
            continue;
        }
        let original = root.join(&relative);
        let sandbox = sandbox_root.join(&relative);
        let original_bytes = fs::read(&original).ok();
        let sandbox_bytes = fs::read(&sandbox).ok();
        if original_bytes != sandbox_bytes {
            changed.push(relative);
        }
    }
    Ok(changed)
}

fn collect_relative_files(
    root: &Path,
    current: &Path,
    paths: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(current)
        .map_err(|err| format!("failed to read directory {}: {err}", current.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".git" | "target" | "node_modules" | ".dbm_autonomous_execute"
        ) {
            continue;
        }
        if path.is_dir() {
            collect_relative_files(root, &path, paths)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?;
            paths.insert(relative.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn sync_changed_file(root: &Path, sandbox_root: &Path, relative: &str) -> Result<(), String> {
    let src = sandbox_root.join(relative);
    let dest = root.join(relative);
    let parent = dest
        .parent()
        .ok_or_else(|| format!("target file has no parent: {}", dest.display()))?;
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create parent directory {}: {err}",
            parent.display()
        )
    })?;
    let contents = fs::read(&src)
        .map_err(|err| format!("failed to read sandbox file {}: {err}", src.display()))?;
    fs::write(&dest, contents)
        .map_err(|err| format!("failed to write project file {}: {err}", dest.display()))
}

fn restore_original_file(
    root: &Path,
    relative: &str,
    original: Option<&[u8]>,
) -> Result<(), String> {
    let dest = root.join(relative);
    match original {
        Some(bytes) => fs::write(&dest, bytes)
            .map_err(|err| format!("failed to restore project file {}: {err}", dest.display())),
        None => {
            if dest.exists() {
                fs::remove_file(&dest).map_err(|err| {
                    format!("failed to remove project file {}: {err}", dest.display())
                })
            } else {
                Ok(())
            }
        }
    }
}

fn diff_between_paths(original: &Path, updated: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["diff", "--no-index", "--"])
        .arg(original)
        .arg(updated)
        .output()
        .map_err(|err| format!("failed to compute dry-run diff: {err}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn diff_stats(diff: &str) -> DiffStats {
    let mut stats = DiffStats::default();
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            stats.lines_added += 1;
        } else if line.starts_with('-') {
            stats.lines_removed += 1;
        }
    }
    stats
}

fn commit_descriptor(attempts: &[ExecuteAttempt]) -> CommitDescriptor<'static> {
    let action = attempts
        .iter()
        .rev()
        .find_map(|attempt| {
            attempt
                .debug
                .as_ref()
                .map(|debug| debug.primary.action.as_str())
        })
        .unwrap_or("fix_compile");
    let kind = match action {
        "install_dependency" => "dependency",
        "add_use" | "add_trait_import" => "import",
        "fix_syntax" => "syntax",
        "fix_borrow" => "borrow",
        _ => "build",
    };
    CommitDescriptor {
        kind,
        detail: action
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub(super) fn commit_message(descriptor: &CommitDescriptor<'_>) -> String {
    let _ = descriptor;
    "auto fix".to_string()
}

fn is_valid_commit_message(message: &str) -> bool {
    message == "auto fix"
}

fn is_allowed_file_type(file: &str) -> bool {
    if file.starts_with(".git/") || file.ends_with(".lock") || file.ends_with(".env") {
        return false;
    }
    matches!(
        Path::new(file).extension().and_then(|ext| ext.to_str()),
        Some("rs" | "ts" | "js" | "toml" | "json")
    )
}

fn verify_post_commit(
    root: &Path,
    tasks: &[String],
    timeout_ms: u64,
) -> Result<Option<String>, String> {
    if tasks.is_empty() {
        return Ok(None);
    }
    let mut adapter = IncidentExecutionAdapter::new(root)?;
    for task in tasks {
        let report = adapter.execute(task, timeout_ms)?;
        if !report.success {
            return Ok(Some(report.error_type));
        }
    }
    Ok(None)
}

fn is_explicit_single_file(path: &str) -> bool {
    !path.is_empty()
        && path != "."
        && !path.contains('*')
        && !path.contains('?')
        && !path.contains('[')
}

pub(super) fn git_command() -> Command {
    let mut command = Command::new("git");
    command.args([
        "-c",
        "diff.external=",
        "-c",
        "core.pager=cat",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
    ]);
    command.env_remove("GIT_EXTERNAL_DIFF");
    command.env_remove("GIT_DIFF_OPTS");
    command.env("GIT_PAGER", "cat");
    command.env("PAGER", "cat");
    command
}

fn git_output_with_retry<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<std::process::Output, std::io::Error> {
    let mut last_error = None;
    for attempt in 0..5 {
        let output = git_command().args(args).current_dir(root).output();
        match output {
            Ok(output) => return Ok(output),
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::ResourceBusy
                ) || err.raw_os_error() == Some(35) =>
            {
                last_error = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1) as u64));
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_error.expect("git retry should capture an error"))
}

pub(super) fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
}

pub(super) fn is_auto_fix_branch(branch: &str) -> bool {
    branch.starts_with("dbm/auto-fix/")
        && branch
            .strip_prefix("dbm/auto-fix/")
            .is_some_and(|suffix| !suffix.trim().is_empty())
}
