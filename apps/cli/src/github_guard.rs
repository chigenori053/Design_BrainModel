use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubCommandKind {
    AuthStatus,
    RepoView,
    PrStatus,
    PrView {
        number: u64,
    },
    PrDiff {
        number: u64,
    },
    IssueView {
        number: u64,
    },
    IssueList,
    PrCreatePreview {
        title: String,
        body: String,
        base: Option<String>,
    },
    PrCreateConfirm {
        token: String,
    },
    Rejected {
        operation: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubCommandOutput {
    schema_version: &'static str,
    status: String,
    operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingGitHubPrCreate {
    schema_version: String,
    operation: String,
    title: String,
    body: String,
    base: String,
    head: String,
    head_commit: String,
    remote: String,
    repo: String,
    confirmation_token: String,
    created_at_ms: u128,
}

impl GitHubCommandOutput {
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

pub fn execute_safe_github_command(
    workspace_root: &Path,
    args: &[String],
) -> (i32, GitHubCommandOutput) {
    let kind = parse_github_args(args);
    let operation = operation_name(&kind);
    match kind {
        GitHubCommandKind::Rejected { reason, .. } => {
            (2, GitHubCommandOutput::rejected(operation, &reason))
        }
        GitHubCommandKind::AuthStatus => run_readonly(operation, &["auth", "status"]),
        GitHubCommandKind::RepoView => {
            run_readonly_authed(operation, &["repo", "view"], workspace_root)
        }
        GitHubCommandKind::PrStatus => {
            run_readonly_authed(operation, &["pr", "status"], workspace_root)
        }
        GitHubCommandKind::PrView { number } => run_readonly_authed(
            operation,
            &["pr", "view", &number.to_string()],
            workspace_root,
        ),
        GitHubCommandKind::PrDiff { number } => run_readonly_authed(
            operation,
            &["pr", "diff", &number.to_string()],
            workspace_root,
        ),
        GitHubCommandKind::IssueView { number } => run_readonly_authed(
            operation,
            &["issue", "view", &number.to_string()],
            workspace_root,
        ),
        GitHubCommandKind::IssueList => {
            run_readonly_authed(operation, &["issue", "list"], workspace_root)
        }
        GitHubCommandKind::PrCreatePreview { title, body, base } => {
            pr_create_preview(workspace_root, operation, &title, &body, base.as_deref())
        }
        GitHubCommandKind::PrCreateConfirm { token } => {
            pr_create_confirm(workspace_root, operation, &token)
        }
    }
}

pub fn parse_github_args(args: &[String]) -> GitHubCommandKind {
    match args {
        [scope, command] if scope == "auth" && command == "status" => GitHubCommandKind::AuthStatus,
        [scope, command] if scope == "repo" && command == "view" => GitHubCommandKind::RepoView,
        [scope, command, ..] if scope == "repo" && command == "edit" => {
            rejected("github", "destructive_github_command_rejected")
        }
        [scope, command] if scope == "pr" && command == "status" => GitHubCommandKind::PrStatus,
        [scope, command, number] if scope == "pr" && command == "view" => parse_number(number)
            .map(|number| GitHubCommandKind::PrView { number })
            .unwrap_or_else(|| rejected("github pr view", "invalid_pr_number")),
        [scope, command, number] if scope == "pr" && command == "diff" => parse_number(number)
            .map(|number| GitHubCommandKind::PrDiff { number })
            .unwrap_or_else(|| rejected("github pr diff", "invalid_pr_number")),
        [scope, command] if scope == "pr" && matches!(command.as_str(), "view" | "diff") => {
            rejected(&format!("github pr {command}"), "missing_pr_number")
        }
        [scope, command, rest @ ..] if scope == "pr" && command == "create" => {
            parse_pr_create(rest)
        }
        [scope, command, ..] if scope == "pr" && is_destructive_pr_command(command) => {
            rejected("github", "destructive_github_command_rejected")
        }
        [scope, command, number] if scope == "issue" && command == "view" => parse_number(number)
            .map(|number| GitHubCommandKind::IssueView { number })
            .unwrap_or_else(|| rejected("github issue view", "invalid_pr_number")),
        [scope, command] if scope == "issue" && command == "view" => {
            rejected("github issue view", "missing_pr_number")
        }
        [scope, command] if scope == "issue" && command == "list" => GitHubCommandKind::IssueList,
        [scope, command, ..]
            if scope == "issue" && matches!(command.as_str(), "close" | "edit") =>
        {
            rejected("github", "destructive_github_command_rejected")
        }
        [scope, ..] if is_destructive_scope(scope) => {
            rejected("github", "destructive_github_command_rejected")
        }
        _ => rejected("github", "unsupported_github_command"),
    }
}

fn parse_pr_create(args: &[String]) -> GitHubCommandKind {
    if let [flag, token] = args
        && flag == "--confirm"
    {
        return GitHubCommandKind::PrCreateConfirm {
            token: token.to_string(),
        };
    }

    let mut title = None;
    let mut body = None;
    let mut base = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--title" => title = iter.next().cloned(),
            "--body" => body = iter.next().cloned(),
            "--base" => base = iter.next().cloned(),
            _ => return rejected("github pr create", "unsupported_github_command"),
        }
    }
    let Some(title) = title.filter(|value| !value.trim().is_empty()) else {
        return rejected("github pr create", "missing_title");
    };
    let Some(body) = body.filter(|value| !value.trim().is_empty()) else {
        return rejected("github pr create", "missing_body");
    };
    GitHubCommandKind::PrCreatePreview { title, body, base }
}

fn pr_create_preview(
    workspace_root: &Path,
    operation: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
) -> (i32, GitHubCommandOutput) {
    if let Err(reason) = ensure_gh_auth(workspace_root) {
        return (2, GitHubCommandOutput::rejected(operation, &reason));
    }
    let branch = match current_branch(workspace_root) {
        Ok(branch) => branch,
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    };
    if matches!(branch.as_str(), "main" | "master") {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "default_branch_pr_rejected"),
        );
    }
    let head_commit = match run_git(workspace_root, &["rev-parse", "HEAD"]) {
        Ok(head) => head.trim().to_string(),
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    };
    if !git_status_clean(workspace_root) {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "working_tree_dirty"),
        );
    }
    if let Ok(upstream_head) = run_git(workspace_root, &["rev-parse", "@{u}"])
        && upstream_head.trim() != head_commit
    {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "unpushed_commits"),
        );
    }
    let repo = match github_repo(workspace_root) {
        Ok(repo) => repo,
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    };
    let remote = git_remote(workspace_root).unwrap_or_else(|_| "origin".to_string());
    let base = base.unwrap_or("main").to_string();
    let created_at_ms = current_time_ms();
    let confirmation_token = pr_create_confirmation_token(
        title,
        body,
        &base,
        &branch,
        &head_commit,
        &repo,
        created_at_ms,
    );
    let pending = PendingGitHubPrCreate {
        schema_version: "v1".to_string(),
        operation: "github_pr_create".to_string(),
        title: title.to_string(),
        body: body.to_string(),
        base,
        head: branch,
        head_commit,
        remote,
        repo,
        confirmation_token,
        created_at_ms,
    };
    if let Err(reason) = write_pending_pr_create(workspace_root, &pending) {
        return (2, GitHubCommandOutput::rejected(operation, &reason));
    }
    (
        0,
        GitHubCommandOutput::confirmation_required(
            operation,
            json!({
                "title": pending.title,
                "body": pending.body,
                "base": pending.base,
                "head": pending.head,
                "confirmation_token": pending.confirmation_token
            }),
        ),
    )
}

fn pr_create_confirm(
    workspace_root: &Path,
    operation: &str,
    token: &str,
) -> (i32, GitHubCommandOutput) {
    let pending = match read_pending_pr_create(workspace_root) {
        Ok(pending) => pending,
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    };
    if pending.operation != "github_pr_create" {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "pending_pr_corrupt"),
        );
    }
    if pending.confirmation_token != token {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "token_mismatch"),
        );
    }
    if let Err(reason) = ensure_gh_auth(workspace_root) {
        let _ = remove_pending_pr_create(workspace_root);
        return (2, GitHubCommandOutput::rejected(operation, &reason));
    }
    match current_branch(workspace_root) {
        Ok(branch) if branch == pending.head => {}
        Ok(_) => {
            let _ = remove_pending_pr_create(workspace_root);
            return (
                2,
                GitHubCommandOutput::rejected(operation, "branch_changed"),
            );
        }
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    }
    match run_git(workspace_root, &["rev-parse", "HEAD"]) {
        Ok(head) if head.trim() == pending.head_commit => {}
        Ok(_) => {
            let _ = remove_pending_pr_create(workspace_root);
            return (2, GitHubCommandOutput::rejected(operation, "head_changed"));
        }
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    }
    if !git_status_clean(workspace_root) {
        return (
            2,
            GitHubCommandOutput::rejected(operation, "working_tree_dirty"),
        );
    }
    match github_repo(workspace_root) {
        Ok(repo) if repo == pending.repo => {}
        Ok(_) => {
            let _ = remove_pending_pr_create(workspace_root);
            return (2, GitHubCommandOutput::rejected(operation, "repo_changed"));
        }
        Err(reason) => return (2, GitHubCommandOutput::rejected(operation, &reason)),
    }
    let output = match run_gh(
        workspace_root,
        &[
            "pr",
            "create",
            "--title",
            &pending.title,
            "--body",
            &pending.body,
            "--base",
            &pending.base,
        ],
    ) {
        Ok(output) => output,
        Err(reason) => return (1, GitHubCommandOutput::rejected(operation, &reason)),
    };
    let _ = remove_pending_pr_create(workspace_root);
    let url = output
        .lines()
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .unwrap_or(output.trim())
        .to_string();
    (
        0,
        GitHubCommandOutput::ok(operation, json!({ "url": url, "output": output })),
    )
}

fn run_readonly(operation: &str, args: &[&str]) -> (i32, GitHubCommandOutput) {
    match run_gh(Path::new("."), args) {
        Ok(output) => (
            0,
            GitHubCommandOutput::ok(operation, json!({ "output": output })),
        ),
        Err(reason) => (2, GitHubCommandOutput::rejected(operation, &reason)),
    }
}

fn run_readonly_authed(
    operation: &str,
    args: &[&str],
    workspace_root: &Path,
) -> (i32, GitHubCommandOutput) {
    if let Err(reason) = ensure_gh_auth(workspace_root) {
        return (2, GitHubCommandOutput::rejected(operation, &reason));
    }
    match run_gh(workspace_root, args) {
        Ok(output) => (
            0,
            GitHubCommandOutput::ok(operation, json!({ "output": output })),
        ),
        Err(reason) => (2, GitHubCommandOutput::rejected(operation, &reason)),
    }
}

fn ensure_gh_auth(workspace_root: &Path) -> Result<(), String> {
    run_gh(workspace_root, &["auth", "status"]).map(|_| ())
}

fn run_gh(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|_| "gh_not_available".to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else if args == ["auth", "status"] {
        Err("gh_auth_required".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "github_command_failed".to_string()
        } else {
            stderr
        })
    }
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
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn current_branch(workspace_root: &Path) -> Result<String, String> {
    let branch = run_git(workspace_root, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if branch == "HEAD" {
        Err("detached_head".to_string())
    } else {
        Ok(branch)
    }
}

fn git_status_clean(workspace_root: &Path) -> bool {
    run_git(workspace_root, &["status", "--porcelain=v1"])
        .map(|status| {
            status
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .all(|line| {
                    line.ends_with(".dbm/") || line.contains(" .dbm/") || line.contains("?? .dbm/")
                })
        })
        .unwrap_or(false)
}

fn github_repo(workspace_root: &Path) -> Result<String, String> {
    let output = run_gh(workspace_root, &["repo", "view", "--json", "nameWithOwner"])?;
    let value: serde_json::Value =
        serde_json::from_str(&output).map_err(|_| "github_command_failed".to_string())?;
    value
        .get("nameWithOwner")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "github_command_failed".to_string())
}

fn git_remote(workspace_root: &Path) -> Result<String, String> {
    Ok(run_git(workspace_root, &["remote"])?
        .lines()
        .next()
        .unwrap_or("origin")
        .to_string())
}

fn pending_pr_create_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    Ok(root.join(".dbm").join("pending_github_pr_create.json"))
}

fn write_pending_pr_create(
    workspace_root: &Path,
    pending: &PendingGitHubPrCreate,
) -> Result<(), String> {
    let path = pending_pr_create_path(workspace_root)?;
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace_root_unavailable".to_string())?;
    if !path.starts_with(&root) {
        return Err("workspace_escape_rejected".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "pending_pr_path_invalid".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("pending_pr_write_failed: {err}"))?;
    let body = serde_json::to_vec_pretty(pending)
        .map_err(|err| format!("pending_pr_write_failed: {err}"))?;
    fs::write(path, body).map_err(|err| format!("pending_pr_write_failed: {err}"))
}

fn read_pending_pr_create(workspace_root: &Path) -> Result<PendingGitHubPrCreate, String> {
    let path = pending_pr_create_path(workspace_root)?;
    let raw = fs::read_to_string(path).map_err(|_| "pending_pr_missing".to_string())?;
    serde_json::from_str(&raw).map_err(|_| "pending_pr_corrupt".to_string())
}

fn remove_pending_pr_create(workspace_root: &Path) -> Result<(), String> {
    let path = pending_pr_create_path(workspace_root)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("pending_pr_remove_failed: {err}")),
    }
}

fn pr_create_confirmation_token(
    title: &str,
    body: &str,
    base: &str,
    head: &str,
    head_commit: &str,
    repo: &str,
    created_at_ms: u128,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        "github_pr_create",
        title,
        body,
        base,
        head,
        head_commit,
        repo,
        &created_at_ms.to_string(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("confirm_{:x}", hasher.finalize())
}

fn current_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn operation_name(kind: &GitHubCommandKind) -> &'static str {
    match kind {
        GitHubCommandKind::AuthStatus => "github_auth_status",
        GitHubCommandKind::RepoView => "github_repo_view",
        GitHubCommandKind::PrStatus => "github_pr_status",
        GitHubCommandKind::PrView { .. } => "github_pr_view",
        GitHubCommandKind::PrDiff { .. } => "github_pr_diff",
        GitHubCommandKind::IssueView { .. } => "github_issue_view",
        GitHubCommandKind::IssueList => "github_issue_list",
        GitHubCommandKind::PrCreatePreview { .. } | GitHubCommandKind::PrCreateConfirm { .. } => {
            "github_pr_create"
        }
        GitHubCommandKind::Rejected { operation, .. }
            if operation.starts_with("github pr create") =>
        {
            "github_pr_create"
        }
        GitHubCommandKind::Rejected { operation, .. }
            if operation.starts_with("github pr view") =>
        {
            "github_pr_view"
        }
        GitHubCommandKind::Rejected { operation, .. }
            if operation.starts_with("github pr diff") =>
        {
            "github_pr_diff"
        }
        GitHubCommandKind::Rejected { operation, .. }
            if operation.starts_with("github issue view") =>
        {
            "github_issue_view"
        }
        GitHubCommandKind::Rejected { .. } => "github",
    }
}

fn rejected(operation: &str, reason: &str) -> GitHubCommandKind {
    GitHubCommandKind::Rejected {
        operation: operation.to_string(),
        reason: reason.to_string(),
    }
}

fn parse_number(raw: &str) -> Option<u64> {
    raw.parse::<u64>().ok().filter(|number| *number > 0)
}

fn is_destructive_pr_command(command: &str) -> bool {
    matches!(command, "merge" | "close")
}

fn is_destructive_scope(scope: &str) -> bool {
    matches!(scope, "release" | "secret" | "workflow")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_auth_status_allowed() {
        assert_eq!(
            parse_github_args(&["auth".to_string(), "status".to_string()]),
            GitHubCommandKind::AuthStatus
        );
    }

    #[test]
    fn github_pr_view_requires_number() {
        assert!(matches!(
            parse_github_args(&["pr".to_string(), "view".to_string()]),
            GitHubCommandKind::Rejected { reason, .. } if reason == "missing_pr_number"
        ));
    }

    #[test]
    fn github_pr_create_requires_title_and_body() {
        assert!(matches!(
            parse_github_args(&["pr".to_string(), "create".to_string(), "--title".to_string(), "x".to_string()]),
            GitHubCommandKind::Rejected { reason, .. } if reason == "missing_body"
        ));
    }

    #[test]
    fn github_destructive_command_rejected() {
        assert!(matches!(
            parse_github_args(&["pr".to_string(), "merge".to_string(), "1".to_string()]),
            GitHubCommandKind::Rejected { reason, .. } if reason == "destructive_github_command_rejected"
        ));
    }
}
