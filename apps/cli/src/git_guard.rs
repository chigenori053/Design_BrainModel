use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const GIT_COMMIT_CONFIRMATION_TOKEN: &str = "DBM_CONFIRM_GIT_COMMIT_V1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCommandKind {
    Status,
    Diff { staged: bool, path: Option<PathBuf> },
    Add { path: PathBuf },
    CommitPreview { message: String },
    CommitConfirm { token: String },
    PushDryRun,
    PushPreview,
    PushConfirm { token: String },
    Rejected { operation: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitSafetyDecision {
    Allow,
    RequireConfirmation { token: String },
    Reject { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeGitCommand {
    pub kind: GitCommandKind,
    pub decision: GitSafetyDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitCommandOutput {
    schema_version: &'static str,
    status: String,
    operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingGitCommit {
    schema_version: String,
    operation: String,
    message: String,
    staged_files: Vec<String>,
    staged_checksum: String,
    confirmation_token: String,
    created_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingGitPush {
    schema_version: String,
    operation: String,
    remote: String,
    branch: String,
    upstream: String,
    head: String,
    ahead_count: u32,
    dry_run_checksum: String,
    confirmation_token: String,
    created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PushSnapshot {
    remote: String,
    branch: String,
    upstream: String,
    head: String,
    ahead_count: u32,
}

impl GitCommandOutput {
    fn ok(operation: &str, data: serde_json::Value) -> Self {
        Self {
            schema_version: "v1",
            status: "ok".to_string(),
            operation: operation.to_string(),
            reason: None,
            data: Some(data),
        }
    }

    fn rejected(operation: &str, reason: &str) -> Self {
        Self {
            schema_version: "v1",
            status: "rejected".to_string(),
            operation: operation.to_string(),
            reason: Some(reason.to_string()),
            data: None,
        }
    }

    fn confirmation_required(operation: &str, data: serde_json::Value) -> Self {
        Self {
            schema_version: "v1",
            status: "confirmation_required".to_string(),
            operation: operation.to_string(),
            reason: None,
            data: Some(data),
        }
    }
}

pub fn normalize_git_nl_input(input: &str) -> Option<Vec<String>> {
    let raw = input.trim();
    let lower = raw.to_ascii_lowercase();
    if raw == "git status を確認" || raw == "git status確認" || raw == "状態を確認" {
        return Some(vec!["status".to_string()]);
    }
    if raw == "差分を確認" || lower == "check diff" {
        return Some(vec!["diff".to_string()]);
    }
    if raw == "push dry-run を実行" || lower == "run push dry-run" {
        return Some(vec!["push".to_string(), "--dry-run".to_string()]);
    }
    if let Some(path) = raw.strip_suffix(" を git add") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(vec!["add".to_string(), path.to_string()]);
        }
    }
    if raw == "コミットして" || lower == "commit changes" {
        return Some(vec!["commit".to_string()]);
    }
    None
}

pub fn parse_git_args(args: &[String]) -> GitCommandKind {
    match args {
        [command] if command == "status" => GitCommandKind::Status,
        [command, rest @ ..] if command == "diff" => parse_diff(rest),
        [command, path] if command == "add" => {
            if is_forbidden_add_scope(path) {
                GitCommandKind::Rejected {
                    operation: format!("git add {path}"),
                    reason: "ambiguous_scope_forbidden".to_string(),
                }
            } else {
                GitCommandKind::Add {
                    path: PathBuf::from(path),
                }
            }
        }
        [command, ..] if command == "add" => GitCommandKind::Rejected {
            operation: "git add".to_string(),
            reason: "ambiguous_scope_forbidden".to_string(),
        },
        [command, rest @ ..] if command == "commit" => parse_commit(rest),
        [command, rest @ ..] if command == "push" => parse_push(rest, args),
        [command, ..] if is_forbidden_git_command(command) => GitCommandKind::Rejected {
            operation: format!("git {}", args.join(" ")),
            reason: "destructive_or_ambiguous_git_operation".to_string(),
        },
        _ => GitCommandKind::Rejected {
            operation: if args.is_empty() {
                "git".to_string()
            } else {
                format!("git {}", args.join(" "))
            },
            reason: "unsupported_git_operation".to_string(),
        },
    }
}

fn parse_push(args: &[String], full_args: &[String]) -> GitCommandKind {
    match args {
        [] => GitCommandKind::PushPreview,
        [flag] if flag == "--dry-run" => GitCommandKind::PushDryRun,
        [flag, token] if flag == "--confirm" => GitCommandKind::PushConfirm {
            token: token.to_string(),
        },
        [flag, ..] if matches!(flag.as_str(), "--force" | "-f" | "--force-with-lease") => {
            GitCommandKind::Rejected {
                operation: format!("git {}", full_args.join(" ")),
                reason: "force_push_rejected".to_string(),
            }
        }
        [flag, ..] if flag == "--set-upstream" || flag == "-u" => GitCommandKind::Rejected {
            operation: format!("git {}", full_args.join(" ")),
            reason: "explicit_remote_branch_not_supported".to_string(),
        },
        [_, ..] => GitCommandKind::Rejected {
            operation: format!("git {}", full_args.join(" ")),
            reason: "explicit_remote_branch_not_supported".to_string(),
        },
    }
}

fn parse_diff(args: &[String]) -> GitCommandKind {
    match args {
        [] => GitCommandKind::Diff {
            staged: false,
            path: None,
        },
        [flag] if is_staged_diff_flag(flag) => GitCommandKind::Diff {
            staged: true,
            path: None,
        },
        [sep, path] if sep == "--" => GitCommandKind::Diff {
            staged: false,
            path: Some(PathBuf::from(path)),
        },
        [flag, sep, path] if is_staged_diff_flag(flag) && sep == "--" => GitCommandKind::Diff {
            staged: true,
            path: Some(PathBuf::from(path)),
        },
        [sep] if sep == "--" => GitCommandKind::Rejected {
            operation: "git diff".to_string(),
            reason: "target_file_missing".to_string(),
        },
        [flag, sep] if is_staged_diff_flag(flag) && sep == "--" => GitCommandKind::Rejected {
            operation: "git diff".to_string(),
            reason: "target_file_missing".to_string(),
        },
        _ => GitCommandKind::Rejected {
            operation: "git diff".to_string(),
            reason: "unsupported_diff_option".to_string(),
        },
    }
}

fn is_staged_diff_flag(flag: &str) -> bool {
    matches!(flag, "--cached" | "--staged")
}

fn parse_commit(args: &[String]) -> GitCommandKind {
    if let [flag, token] = args
        && flag == "--confirm"
    {
        return GitCommandKind::CommitConfirm {
            token: token.to_string(),
        };
    }

    let mut message = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-m" | "--message" => {
                message = iter.next().cloned();
            }
            _ => {
                return GitCommandKind::Rejected {
                    operation: "git commit".to_string(),
                    reason: "unsupported_git_operation".to_string(),
                };
            }
        }
    }
    let Some(message) = message else {
        return GitCommandKind::Rejected {
            operation: "git commit".to_string(),
            reason: "empty_commit_message".to_string(),
        };
    };
    if message.trim().is_empty() {
        return GitCommandKind::Rejected {
            operation: "git commit".to_string(),
            reason: "empty_commit_message".to_string(),
        };
    }
    GitCommandKind::CommitPreview { message }
}

pub fn guard_git_command(workspace_root: &Path, kind: GitCommandKind) -> SafeGitCommand {
    let decision = match &kind {
        GitCommandKind::Status | GitCommandKind::PushDryRun => GitSafetyDecision::Allow,
        GitCommandKind::Diff { path: None, .. } => GitSafetyDecision::Allow,
        GitCommandKind::Diff {
            path: Some(path), ..
        } => guard_existing_workspace_path(workspace_root, path)
            .map(|_| GitSafetyDecision::Allow)
            .unwrap_or_else(|reason| GitSafetyDecision::Reject { reason }),
        GitCommandKind::Add { path } => {
            if has_glob_pattern(path) || path.is_absolute() {
                GitSafetyDecision::Reject {
                    reason: "ambiguous_scope_forbidden".to_string(),
                }
            } else {
                guard_existing_workspace_path(workspace_root, path)
                    .map(|_| GitSafetyDecision::Allow)
                    .unwrap_or_else(|reason| GitSafetyDecision::Reject { reason })
            }
        }
        GitCommandKind::CommitPreview { .. } | GitCommandKind::CommitConfirm { .. } => {
            GitSafetyDecision::Allow
        }
        GitCommandKind::PushPreview | GitCommandKind::PushConfirm { .. } => {
            GitSafetyDecision::Allow
        }
        GitCommandKind::Rejected { reason, .. } => GitSafetyDecision::Reject {
            reason: reason.clone(),
        },
    };
    SafeGitCommand { kind, decision }
}

pub fn execute_safe_git_command(workspace_root: &Path, args: &[String]) -> (i32, GitCommandOutput) {
    let args = normalize_git_nl_args(args);
    let safe = guard_git_command(workspace_root, parse_git_args(&args));
    let operation = operation_name(&safe.kind);
    match (&safe.kind, &safe.decision) {
        (_, GitSafetyDecision::Reject { reason }) => {
            (2, GitCommandOutput::rejected(operation, reason))
        }
        (_, GitSafetyDecision::RequireConfirmation { .. }) => (
            2,
            GitCommandOutput::rejected(operation, "confirmation_required"),
        ),
        (GitCommandKind::Status, GitSafetyDecision::Allow) => {
            match run_git(workspace_root, &["status", "--porcelain=v1"]) {
                Ok(porcelain) => (
                    0,
                    GitCommandOutput::ok(operation, json!({ "porcelain": porcelain })),
                ),
                Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
            }
        }
        (GitCommandKind::Diff { staged, path }, GitSafetyDecision::Allow) => {
            let output = if let Some(path) = path {
                let prefix = if *staged {
                    ["diff", "--cached", "--"].as_slice()
                } else {
                    ["diff", "--"].as_slice()
                };
                run_git_os(workspace_root, prefix, Some(path))
            } else if *staged {
                run_git(workspace_root, &["diff", "--cached"])
            } else {
                run_git(workspace_root, &["diff"])
            };
            match output {
                Ok(diff) => (0, GitCommandOutput::ok(operation, json!({ "diff": diff }))),
                Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
            }
        }
        (GitCommandKind::Add { path }, GitSafetyDecision::Allow) => {
            match run_git_os(workspace_root, ["add", "--"].as_slice(), Some(path)) {
                Ok(_) => (
                    0,
                    GitCommandOutput::ok(operation, json!({ "path": path.display().to_string() })),
                ),
                Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
            }
        }
        (GitCommandKind::CommitPreview { message }, GitSafetyDecision::Allow) => {
            commit_preview(workspace_root, operation, message)
        }
        (GitCommandKind::CommitConfirm { token }, GitSafetyDecision::Allow) => {
            commit_confirm(workspace_root, operation, token)
        }
        (GitCommandKind::PushDryRun, GitSafetyDecision::Allow) => {
            match run_git(workspace_root, &["push", "--dry-run"]) {
                Ok(output) => (
                    0,
                    GitCommandOutput::ok(operation, json!({ "output": output })),
                ),
                Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
            }
        }
        (GitCommandKind::PushPreview, GitSafetyDecision::Allow) => {
            push_preview(workspace_root, operation)
        }
        (GitCommandKind::PushConfirm { token }, GitSafetyDecision::Allow) => {
            push_confirm(workspace_root, operation, token)
        }
        _ => (
            2,
            GitCommandOutput::rejected(operation, "unsupported_git_operation"),
        ),
    }
}

fn normalize_git_nl_args(args: &[String]) -> Vec<String> {
    if args.len() == 1 {
        normalize_git_nl_input(&args[0]).unwrap_or_else(|| args.to_vec())
    } else {
        args.to_vec()
    }
}

fn commit_preview(
    workspace_root: &Path,
    operation: &str,
    message: &str,
) -> (i32, GitCommandOutput) {
    match staged_snapshot(workspace_root) {
        Ok((staged_files, staged_diff, staged_checksum)) if !staged_files.is_empty() => {
            let created_at_ms = current_time_ms();
            let confirmation_token =
                confirmation_token(message, &staged_checksum, workspace_root, created_at_ms);
            let pending = PendingGitCommit {
                schema_version: "v1".to_string(),
                operation: "git_commit".to_string(),
                message: message.to_string(),
                staged_files,
                staged_checksum,
                confirmation_token,
                created_at_ms,
            };
            if let Err(reason) = write_pending_commit(workspace_root, &pending) {
                return (2, GitCommandOutput::rejected(operation, &reason));
            }
            (
                0,
                GitCommandOutput::confirmation_required(
                    operation,
                    json!({
                        "message": pending.message,
                        "staged_files": pending.staged_files,
                        "staged_checksum": pending.staged_checksum,
                        "confirmation_token": pending.confirmation_token,
                        "diff_bytes": staged_diff.len()
                    }),
                ),
            )
        }
        Ok(_) => (2, GitCommandOutput::rejected(operation, "nothing_staged")),
        Err(reason) => (2, GitCommandOutput::rejected(operation, &reason)),
    }
}

fn commit_confirm(workspace_root: &Path, operation: &str, token: &str) -> (i32, GitCommandOutput) {
    let pending = match read_pending_commit(workspace_root) {
        Ok(pending) => pending,
        Err(reason) => return (2, GitCommandOutput::rejected(operation, &reason)),
    };
    if pending.operation != "git_commit" {
        return (
            2,
            GitCommandOutput::rejected(operation, "pending_commit_corrupt"),
        );
    }
    if pending.confirmation_token != token {
        return (
            2,
            GitCommandOutput::rejected(operation, "confirmation_token_mismatch"),
        );
    }
    match staged_snapshot(workspace_root) {
        Ok((files, _, _)) if files.is_empty() => {
            let _ = remove_pending_commit(workspace_root);
            (2, GitCommandOutput::rejected(operation, "nothing_staged"))
        }
        Ok((_, _, checksum)) if checksum != pending.staged_checksum => {
            let _ = remove_pending_commit(workspace_root);
            (
                2,
                GitCommandOutput::rejected(operation, "staged_diff_changed"),
            )
        }
        Ok((files, _, _)) => {
            match run_git(workspace_root, &["commit", "-m", pending.message.as_str()]) {
                Ok(output) => {
                    let _ = remove_pending_commit(workspace_root);
                    match run_git(workspace_root, &["rev-parse", "--short", "HEAD"]) {
                        Ok(hash) => (
                            0,
                            GitCommandOutput::ok(
                                operation,
                                json!({
                                    "message": pending.message,
                                    "files": files,
                                    "commit": hash.trim(),
                                    "output": output
                                }),
                            ),
                        ),
                        Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
                    }
                }
                Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
            }
        }
        Err(reason) => (2, GitCommandOutput::rejected(operation, &reason)),
    }
}

fn push_preview(workspace_root: &Path, operation: &str) -> (i32, GitCommandOutput) {
    let snapshot = match push_snapshot(workspace_root) {
        Ok(snapshot) => snapshot,
        Err(reason) => return (2, GitCommandOutput::rejected(operation, &reason)),
    };
    if snapshot.ahead_count == 0 {
        return (2, GitCommandOutput::rejected(operation, "nothing_to_push"));
    }
    let dry_run_output = match run_git(workspace_root, &["push", "--dry-run"]) {
        Ok(output) => output,
        Err(_) => return (2, GitCommandOutput::rejected(operation, "dry_run_failed")),
    };
    let dry_run_checksum = sha256_hex(dry_run_output.as_bytes());
    let created_at_ms = current_time_ms();
    let confirmation_token = push_confirmation_token(&snapshot, &dry_run_checksum, created_at_ms);
    let pending = PendingGitPush {
        schema_version: "v1".to_string(),
        operation: "git_push".to_string(),
        remote: snapshot.remote.clone(),
        branch: snapshot.branch.clone(),
        upstream: snapshot.upstream.clone(),
        head: snapshot.head.clone(),
        ahead_count: snapshot.ahead_count,
        dry_run_checksum,
        confirmation_token,
        created_at_ms,
    };
    if let Err(reason) = write_pending_push(workspace_root, &pending) {
        return (2, GitCommandOutput::rejected(operation, &reason));
    }
    (
        0,
        GitCommandOutput::confirmation_required(
            operation,
            json!({
                "remote": pending.remote,
                "branch": pending.branch,
                "upstream": pending.upstream,
                "head": pending.head,
                "ahead_count": pending.ahead_count,
                "dry_run_output": dry_run_output,
                "confirmation_token": pending.confirmation_token,
            }),
        ),
    )
}

fn push_confirm(workspace_root: &Path, operation: &str, token: &str) -> (i32, GitCommandOutput) {
    let pending = match read_pending_push(workspace_root) {
        Ok(pending) => pending,
        Err(reason) => return (2, GitCommandOutput::rejected(operation, &reason)),
    };
    if pending.operation != "git_push" {
        return (
            2,
            GitCommandOutput::rejected(operation, "pending_push_corrupt"),
        );
    }
    if pending.confirmation_token != token {
        return (2, GitCommandOutput::rejected(operation, "token_mismatch"));
    }
    let snapshot = match push_snapshot(workspace_root) {
        Ok(snapshot) => snapshot,
        Err(reason) => {
            let _ = remove_pending_push(workspace_root);
            return (2, GitCommandOutput::rejected(operation, &reason));
        }
    };
    if snapshot.branch != pending.branch {
        let _ = remove_pending_push(workspace_root);
        return (2, GitCommandOutput::rejected(operation, "branch_changed"));
    }
    if snapshot.upstream != pending.upstream {
        let _ = remove_pending_push(workspace_root);
        return (2, GitCommandOutput::rejected(operation, "upstream_changed"));
    }
    if snapshot.head != pending.head {
        let _ = remove_pending_push(workspace_root);
        return (2, GitCommandOutput::rejected(operation, "head_changed"));
    }
    if snapshot.ahead_count != pending.ahead_count {
        let _ = remove_pending_push(workspace_root);
        return (
            2,
            GitCommandOutput::rejected(operation, "ahead_count_changed"),
        );
    }
    if run_git(workspace_root, &["push", "--dry-run"]).is_err() {
        return (2, GitCommandOutput::rejected(operation, "dry_run_failed"));
    }
    match run_git(workspace_root, &["push"]) {
        Ok(output) => {
            let _ = remove_pending_push(workspace_root);
            (
                0,
                GitCommandOutput::ok(
                    operation,
                    json!({
                        "remote": pending.remote,
                        "branch": pending.branch,
                        "upstream": pending.upstream,
                        "output": output,
                    }),
                ),
            )
        }
        Err(reason) => (1, GitCommandOutput::rejected(operation, &reason)),
    }
}

fn push_snapshot(workspace_root: &Path) -> Result<PushSnapshot, String> {
    let branch = run_git(workspace_root, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if branch == "HEAD" {
        return Err("detached_head".to_string());
    }
    let upstream = run_git(
        workspace_root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .map_err(|_| "upstream_not_configured".to_string())?
    .trim()
    .to_string();
    let remote = upstream
        .split_once('/')
        .map(|(remote, _)| remote.to_string())
        .filter(|remote| !remote.is_empty())
        .unwrap_or_else(|| "origin".to_string());
    let head = run_git(workspace_root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let ahead_count = run_git(workspace_root, &["rev-list", "--count", "@{u}..HEAD"])?
        .trim()
        .parse::<u32>()
        .map_err(|_| "ahead_count_unavailable".to_string())?;
    Ok(PushSnapshot {
        remote,
        branch,
        upstream,
        head,
        ahead_count,
    })
}

fn staged_snapshot(workspace_root: &Path) -> Result<(Vec<String>, String, String), String> {
    let staged_files = staged_files(workspace_root)?;
    let staged_diff = run_git(workspace_root, &["diff", "--cached"])?;
    let staged_checksum = sha256_hex(staged_diff.as_bytes());
    Ok((staged_files, staged_diff, staged_checksum))
}

fn staged_files(workspace_root: &Path) -> Result<Vec<String>, String> {
    Ok(
        run_git(workspace_root, &["diff", "--cached", "--name-only"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn pending_commit_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    Ok(root.join(".dbm").join("pending_git_commit.json"))
}

fn write_pending_commit(workspace_root: &Path, pending: &PendingGitCommit) -> Result<(), String> {
    let path = pending_commit_path(workspace_root)?;
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    if !path.starts_with(&root) {
        return Err("workspace_escape_rejected".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "pending_commit_path_invalid".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("pending_commit_write_failed: {err}"))?;
    let body = serde_json::to_vec_pretty(pending)
        .map_err(|err| format!("pending_commit_write_failed: {err}"))?;
    fs::write(path, body).map_err(|err| format!("pending_commit_write_failed: {err}"))
}

fn read_pending_commit(workspace_root: &Path) -> Result<PendingGitCommit, String> {
    let path = pending_commit_path(workspace_root)?;
    let raw = fs::read_to_string(path).map_err(|_| "pending_commit_not_found".to_string())?;
    serde_json::from_str(&raw).map_err(|_| "pending_commit_corrupt".to_string())
}

fn remove_pending_commit(workspace_root: &Path) -> Result<(), String> {
    let path = pending_commit_path(workspace_root)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("pending_commit_remove_failed: {err}")),
    }
}

fn pending_push_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    Ok(root.join(".dbm").join("pending_git_push.json"))
}

fn write_pending_push(workspace_root: &Path, pending: &PendingGitPush) -> Result<(), String> {
    let path = pending_push_path(workspace_root)?;
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    if !path.starts_with(&root) {
        return Err("workspace_escape_rejected".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "pending_push_path_invalid".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("pending_push_write_failed: {err}"))?;
    let body = serde_json::to_vec_pretty(pending)
        .map_err(|err| format!("pending_push_write_failed: {err}"))?;
    fs::write(path, body).map_err(|err| format!("pending_push_write_failed: {err}"))
}

fn read_pending_push(workspace_root: &Path) -> Result<PendingGitPush, String> {
    let path = pending_push_path(workspace_root)?;
    let raw = fs::read_to_string(path).map_err(|_| "pending_push_missing".to_string())?;
    serde_json::from_str(&raw).map_err(|_| "pending_push_corrupt".to_string())
}

fn remove_pending_push(workspace_root: &Path) -> Result<(), String> {
    let path = pending_push_path(workspace_root)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("pending_push_remove_failed: {err}")),
    }
}

fn confirmation_token(
    message: &str,
    staged_checksum: &str,
    workspace_root: &Path,
    created_at_ms: u128,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"git_commit");
    hasher.update([0]);
    hasher.update(message.as_bytes());
    hasher.update([0]);
    hasher.update(staged_checksum.as_bytes());
    hasher.update([0]);
    hasher.update(workspace_root.display().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(created_at_ms.to_string().as_bytes());
    format!("confirm_{:x}", hasher.finalize())
}

fn push_confirmation_token(
    snapshot: &PushSnapshot,
    dry_run_checksum: &str,
    created_at_ms: u128,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"git_push");
    hasher.update([0]);
    hasher.update(snapshot.remote.as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.branch.as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.upstream.as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.head.as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.ahead_count.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(dry_run_checksum.as_bytes());
    hasher.update([0]);
    hasher.update(created_at_ms.to_string().as_bytes());
    format!("confirm_{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn current_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn operation_name(kind: &GitCommandKind) -> &'static str {
    match kind {
        GitCommandKind::Status => "git_status",
        GitCommandKind::Diff { .. } => "git_diff",
        GitCommandKind::Add { .. } => "git_add",
        GitCommandKind::CommitPreview { .. } | GitCommandKind::CommitConfirm { .. } => "git_commit",
        GitCommandKind::PushDryRun => "git_push_dry_run",
        GitCommandKind::PushPreview | GitCommandKind::PushConfirm { .. } => "git_push",
        GitCommandKind::Rejected { operation, .. } if operation.starts_with("git add") => "git_add",
        GitCommandKind::Rejected { operation, .. } if operation.starts_with("git diff") => {
            "git_diff"
        }
        GitCommandKind::Rejected { operation, .. } if operation.starts_with("git push") => {
            "git_push"
        }
        GitCommandKind::Rejected { operation, .. } if operation.starts_with("git commit") => {
            "git_commit"
        }
        GitCommandKind::Rejected { .. } => "git",
    }
}

fn guard_existing_workspace_path(workspace_root: &Path, path: &Path) -> Result<PathBuf, String> {
    if has_parent_component(path) {
        return Err("workspace_escape_rejected".to_string());
    }
    if has_glob_pattern(path) {
        return Err("ambiguous_scope_forbidden".to_string());
    }
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "target_file_missing".to_string())?;
    if !canonical.starts_with(&root) {
        return Err("workspace_escape_rejected".to_string());
    }
    if !canonical.is_file() {
        return Err("target_file_missing".to_string());
    }
    Ok(canonical)
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn has_glob_pattern(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains('*') || text.contains('?') || text.contains('[') || text.contains(']')
}

fn is_forbidden_add_scope(path: &str) -> bool {
    matches!(path, "." | "-A" | "--all") || has_glob_pattern(Path::new(path))
}

fn is_forbidden_git_command(command: &str) -> bool {
    matches!(
        command,
        "reset" | "clean" | "checkout" | "restore" | "rebase" | "merge" | "branch" | "tag"
    )
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed_to_run_git: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "git_command_failed".to_string()
        } else {
            stderr
        })
    }
}

fn run_git_os(root: &Path, prefix: &[&str], path: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new("git");
    command.args(prefix).current_dir(root);
    if let Some(path) = path {
        command.arg(path);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed_to_run_git: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "git_command_failed".to_string()
        } else {
            stderr
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_test_name(prefix: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        )
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("dbm_git_guard_{}", unique_test_name(name)));
        std::fs::create_dir_all(root.join("apps/cli/src")).expect("mkdir");
        std::fs::write(root.join("apps/cli/src/main.rs"), "fn main() {}\n").expect("write");
        root
    }

    fn decision(root: &Path, args: &[&str]) -> GitSafetyDecision {
        guard_git_command(
            root,
            parse_git_args(
                &args
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>(),
            ),
        )
        .decision
    }

    fn temp_git_repo(name: &str) -> PathBuf {
        let root = temp_workspace(name);
        run_git(&root, &["init"]).expect("git init");
        run_git(&root, &["config", "user.name", "DBM CLI Test"]).expect("git config name");
        run_git(
            &root,
            &["config", "user.email", "dbm-cli-test@example.invalid"],
        )
        .expect("git config email");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");
        run_git(&root, &["commit", "-m", "initial"]).expect("git commit");
        root
    }

    fn temp_git_repo_with_upstream(name: &str) -> (PathBuf, PathBuf) {
        let root = temp_git_repo(name);
        let remote = root.with_extension("remote.git");
        run_git(
            remote.parent().expect("parent"),
            &["init", "--bare", remote.to_str().expect("remote")],
        )
        .expect("git init bare");
        run_git(
            &root,
            &["remote", "add", "origin", remote.to_str().expect("remote")],
        )
        .expect("git remote add");
        run_git(&root, &["push", "-u", "origin", "HEAD"]).expect("initial push");
        (root, remote)
    }

    fn create_ahead_commit(root: &Path, marker: &str) {
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            format!("fn main() {{ println!(\"{marker}\"); }}\n"),
        )
        .expect("write");
        run_git(root, &["add", "apps/cli/src/main.rs"]).expect("git add");
        run_git(root, &["commit", "-m", marker]).expect("git commit");
    }

    fn execute(root: &Path, args: &[&str]) -> (i32, GitCommandOutput) {
        execute_safe_git_command(
            root,
            &args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
        )
    }

    struct PushPreviewFixture {
        root: PathBuf,
        remote: PathBuf,
        token: String,
        pending_path: PathBuf,
    }

    impl PushPreviewFixture {
        fn prepare(prefix: &str, marker: &str) -> Self {
            let (root, remote) = temp_git_repo_with_upstream(prefix);
            create_ahead_commit(&root, marker);
            let (_, preview) = execute(&root, &["push"]);
            let token = preview.data.as_ref().expect("data")["confirmation_token"]
                .as_str()
                .expect("token")
                .to_string();
            let pending_path = pending_push_path(&root).expect("pending path");
            Self {
                root,
                remote,
                token,
                pending_path,
            }
        }
    }

    #[test]
    fn git_guard_allows_status() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("status");
        assert_eq!(decision(&root, &["status"]), GitSafetyDecision::Allow);
    }

    #[test]
    fn git_guard_allows_diff() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("diff");
        assert_eq!(decision(&root, &["diff"]), GitSafetyDecision::Allow);
    }

    #[test]
    fn git_guard_allows_staged_diff() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("staged_diff");
        assert_eq!(
            decision(&root, &["diff", "--cached"]),
            GitSafetyDecision::Allow
        );
        assert_eq!(
            decision(&root, &["diff", "--staged"]),
            GitSafetyDecision::Allow
        );
    }

    #[test]
    fn git_guard_allows_staged_diff_with_explicit_path() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("staged_diff_path");
        assert_eq!(
            decision(&root, &["diff", "--cached", "--", "apps/cli/src/main.rs"]),
            GitSafetyDecision::Allow
        );
        assert_eq!(
            decision(&root, &["diff", "--staged", "--", "apps/cli/src/main.rs"]),
            GitSafetyDecision::Allow
        );
    }

    #[test]
    fn git_guard_rejects_unknown_diff_option() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("diff_unknown");
        assert_eq!(
            decision(&root, &["diff", "--name-only"]),
            GitSafetyDecision::Reject {
                reason: "unsupported_diff_option".to_string()
            }
        );
    }

    #[test]
    fn git_guard_allows_explicit_add() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("add_file");
        assert_eq!(
            decision(&root, &["add", "apps/cli/src/main.rs"]),
            GitSafetyDecision::Allow
        );
    }

    #[test]
    fn git_guard_rejects_add_dot() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("add_dot");
        assert!(matches!(
            decision(&root, &["add", "."]),
            GitSafetyDecision::Reject { .. }
        ));
    }

    #[test]
    fn git_guard_rejects_add_all() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("add_all");
        assert!(matches!(
            decision(&root, &["add", "-A"]),
            GitSafetyDecision::Reject { .. }
        ));
        assert!(matches!(
            decision(&root, &["add", "--all"]),
            GitSafetyDecision::Reject { .. }
        ));
    }

    #[test]
    fn git_guard_rejects_workspace_escape() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("escape");
        assert!(matches!(
            decision(&root, &["add", "../outside.rs"]),
            GitSafetyDecision::Reject { .. }
        ));
    }

    #[test]
    fn git_guard_requires_commit_confirmation() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("commit_confirm");
        assert_eq!(
            decision(&root, &["commit", "-m", "message"]),
            GitSafetyDecision::Allow
        );
    }

    #[test]
    fn git_commit_empty_message_rejected() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("commit_empty_message");
        let (_, output) = execute(&root, &["commit", "-m", "   "]);

        assert_eq!(output.status, "rejected");
        assert_eq!(output.operation, "git_commit");
        assert_eq!(output.reason.as_deref(), Some("empty_commit_message"));
    }

    #[test]
    fn git_commit_without_staged_files_rejected() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_git_repo("commit_nothing_staged");
        let (_, output) = execute(&root, &["commit", "-m", "nothing staged"]);

        assert_eq!(output.status, "rejected");
        assert_eq!(output.reason.as_deref(), Some("nothing_staged"));
    }

    #[test]
    fn git_commit_preview_creates_pending_confirmation() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_git_repo("commit_preview");
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            "fn main() { println!(\"preview\"); }\n",
        )
        .expect("write");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");

        let (_, output) = execute(&root, &["commit", "-m", "preview commit"]);

        let data = output.data.as_ref().expect("data");
        assert_eq!(output.status, "confirmation_required");
        assert_eq!(data["message"], "preview commit");
        assert_eq!(data["staged_files"][0], "apps/cli/src/main.rs");
        assert!(
            data["staged_checksum"]
                .as_str()
                .is_some_and(|v| !v.is_empty())
        );
        assert!(
            data["confirmation_token"]
                .as_str()
                .is_some_and(|v| v.starts_with("confirm_"))
        );
        assert!(pending_commit_path(&root).expect("pending path").exists());
    }

    #[test]
    fn git_commit_confirm_rejects_wrong_token() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_git_repo("commit_wrong_token");
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            "fn main() { println!(\"wrong token\"); }\n",
        )
        .expect("write");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");
        execute(&root, &["commit", "-m", "wrong token"]);

        let (_, output) = execute(&root, &["commit", "--confirm", "confirm_wrong"]);

        assert_eq!(output.status, "rejected");
        assert_eq!(
            output.reason.as_deref(),
            Some("confirmation_token_mismatch")
        );
        assert!(pending_commit_path(&root).expect("pending path").exists());
    }

    #[test]
    fn git_commit_confirm_rejects_stale_staged_diff() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_git_repo("commit_stale");
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            "fn main() { println!(\"first\"); }\n",
        )
        .expect("write");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");
        let (_, preview) = execute(&root, &["commit", "-m", "stale commit"]);
        let token = preview.data.as_ref().expect("data")["confirmation_token"]
            .as_str()
            .expect("token")
            .to_string();
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            "fn main() { println!(\"second\"); }\n",
        )
        .expect("write");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");

        let (_, output) = execute(&root, &["commit", "--confirm", &token]);

        assert_eq!(output.status, "rejected");
        assert_eq!(output.reason.as_deref(), Some("staged_diff_changed"));
        assert!(!pending_commit_path(&root).expect("pending path").exists());
    }

    #[test]
    fn git_commit_confirm_executes_when_checksum_matches() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_git_repo("commit_confirm_executes");
        std::fs::write(
            root.join("apps/cli/src/main.rs"),
            "fn main() { println!(\"confirmed\"); }\n",
        )
        .expect("write");
        run_git(&root, &["add", "apps/cli/src/main.rs"]).expect("git add");
        let (_, preview) = execute(&root, &["commit", "-m", "confirmed commit"]);
        let token = preview.data.as_ref().expect("data")["confirmation_token"]
            .as_str()
            .expect("token")
            .to_string();

        let (_, output) = execute(&root, &["commit", "--confirm", &token]);

        assert_eq!(output.status, "ok");
        assert_eq!(output.operation, "git_commit");
        assert!(!pending_commit_path(&root).expect("pending path").exists());
        let log = run_git(&root, &["log", "-1", "--pretty=%s"]).expect("git log");
        assert_eq!(log.trim(), "confirmed commit");
    }

    #[test]
    fn git_guard_rejects_push_without_dry_run() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("push");
        assert_eq!(decision(&root, &["push"]), GitSafetyDecision::Allow);
    }

    #[test]
    fn git_guard_allows_push_dry_run() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("push_dry_run");
        assert_eq!(
            decision(&root, &["push", "--dry-run"]),
            GitSafetyDecision::Allow
        );
    }

    #[test]
    fn git_guard_rejects_force_push() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("force_push");
        assert_eq!(
            decision(&root, &["push", "--force"]),
            GitSafetyDecision::Reject {
                reason: "force_push_rejected".to_string()
            }
        );
        assert_eq!(
            decision(&root, &["push", "-f"]),
            GitSafetyDecision::Reject {
                reason: "force_push_rejected".to_string()
            }
        );
        assert_eq!(
            decision(&root, &["push", "--force-with-lease"]),
            GitSafetyDecision::Reject {
                reason: "force_push_rejected".to_string()
            }
        );
    }

    #[test]
    fn git_push_explicit_remote_branch_rejected() {
        let _guard = crate::test_support::git_guard_lock();
        let root = temp_workspace("push_explicit");
        assert_eq!(
            decision(&root, &["push", "origin", "main"]),
            GitSafetyDecision::Reject {
                reason: "explicit_remote_branch_not_supported".to_string()
            }
        );
        assert_eq!(
            decision(&root, &["push", "--set-upstream", "origin", "main"]),
            GitSafetyDecision::Reject {
                reason: "explicit_remote_branch_not_supported".to_string()
            }
        );
    }

    #[test]
    fn git_push_preview_requires_confirmation() {
        let _guard = crate::test_support::git_guard_lock();
        let fixture = PushPreviewFixture::prepare("push_preview", "push preview");

        assert!(fixture.token.starts_with("confirm_"));
        assert!(fixture.pending_path.exists());
    }

    #[test]
    fn git_push_confirm_rejects_wrong_token() {
        let _guard = crate::test_support::git_guard_lock();
        let fixture = PushPreviewFixture::prepare("push_wrong_token", "push wrong token");

        let (_, output) = execute(&fixture.root, &["push", "--confirm", "confirm_wrong"]);

        assert_eq!(output.status, "rejected");
        assert_eq!(output.reason.as_deref(), Some("token_mismatch"));
        assert!(fixture.pending_path.exists());
    }

    #[test]
    fn git_push_confirm_rejects_head_changed() {
        let _guard = crate::test_support::git_guard_lock();
        let fixture = PushPreviewFixture::prepare("push_head_changed", "push first");
        create_ahead_commit(&fixture.root, "push second");

        let (_, output) = execute(&fixture.root, &["push", "--confirm", &fixture.token]);

        assert_eq!(output.status, "rejected");
        assert_eq!(output.reason.as_deref(), Some("head_changed"));
        assert!(!fixture.pending_path.exists());
    }

    #[test]
    fn git_push_success_removes_pending_file() {
        let _guard = crate::test_support::git_guard_lock();
        let fixture = PushPreviewFixture::prepare("push_success", "push success");

        let (_, output) = execute(&fixture.root, &["push", "--confirm", &fixture.token]);

        assert_eq!(output.status, "ok");
        assert_eq!(output.operation, "git_push");
        assert!(!fixture.pending_path.exists());
        let local_head = run_git(&fixture.root, &["rev-parse", "HEAD"]).expect("local head");
        let remote_head = run_git(&fixture.remote, &["rev-parse", "HEAD"]).expect("remote head");
        assert_eq!(local_head.trim(), remote_head.trim());
    }
}
