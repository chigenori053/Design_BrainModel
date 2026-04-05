use super::git_integration::{GitExecutor, git_command, is_auto_fix_branch};
use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RemoteAction {
    pub r#type: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RemoteIntegrationReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub pushed: bool,
    pub pr_created: bool,
    pub pr_duplicate: bool,
    pub dry_run: bool,
    pub confirmation_required: bool,
    pub confirmation_granted: bool,
    pub actions: Vec<RemoteAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
struct RemoteTelemetry {
    remote: RemoteTelemetryData,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
struct RemoteTelemetryData {
    branch: String,
    dry_run_ok: bool,
    push_ok: bool,
    pr_created: bool,
    pr_duplicate: bool,
    base: String,
    remote: String,
}

pub(super) fn finalize_remote_integration<C: ConfirmationHandler>(
    root: &Path,
    attempts: &[ExecuteAttempt],
    git: Option<&GitIntegrationReport>,
    options: &GitIntegrationOptions,
    confirmer: &mut C,
) -> Result<Option<RemoteIntegrationReport>, String> {
    RemoteIntegration::finalize(root, attempts, git, options, confirmer)
}

pub(super) struct RemoteIntegration;

impl RemoteIntegration {
    pub(super) fn finalize<C: ConfirmationHandler>(
        root: &Path,
        attempts: &[ExecuteAttempt],
        git: Option<&GitIntegrationReport>,
        options: &GitIntegrationOptions,
        confirmer: &mut C,
    ) -> Result<Option<RemoteIntegrationReport>, String> {
        if !options.enable_remote {
            return Ok(None);
        }
        let Some(git) = git else {
            return Ok(None);
        };
        if !git.committed || git.rolled_back {
            return Ok(None);
        }
        if !index_is_clean(root)? {
            return Ok(Some(RemoteIntegrationReport {
                branch: None,
                base_branch: Some("main".to_string()),
                pushed: false,
                pr_created: false,
                pr_duplicate: false,
                dry_run: options.dry_run,
                confirmation_required: false,
                confirmation_granted: false,
                actions: Vec::new(),
                pr_url: None,
                reason: Some("dirty_index_after_commit".to_string()),
                telemetry_path: None,
            }));
        }

        if let Some(reason) = AuthValidator::validate(root)? {
            let telemetry_path = persist_remote_telemetry(
                root,
                &RemoteTelemetry {
                    remote: RemoteTelemetryData {
                        branch: current_branch(root)?.unwrap_or_default(),
                        dry_run_ok: false,
                        push_ok: false,
                        pr_created: false,
                        pr_duplicate: false,
                        base: "main".to_string(),
                        remote: "origin".to_string(),
                    },
                },
            )?;
            return Ok(Some(RemoteIntegrationReport {
                branch: None,
                base_branch: Some("main".to_string()),
                pushed: false,
                pr_created: false,
                pr_duplicate: false,
                dry_run: options.dry_run,
                confirmation_required: false,
                confirmation_granted: false,
                actions: Vec::new(),
                pr_url: None,
                reason: Some(reason),
                telemetry_path: Some(telemetry_path),
            }));
        }

        let branch = BranchManager::create(root)?;
        let base_branch = "main".to_string();
        let mut actions = vec![RemoteAction {
            r#type: "branch".to_string(),
            target: branch.clone(),
        }];

        PushController::push(root, &branch, true)?;
        actions.push(RemoteAction {
            r#type: "push_dry_run".to_string(),
            target: format!("origin/{branch}"),
        });

        if options.dry_run {
            let telemetry_path = persist_remote_telemetry(
                root,
                &RemoteTelemetry {
                    remote: RemoteTelemetryData {
                        branch: branch.clone(),
                        dry_run_ok: true,
                        push_ok: false,
                        pr_created: false,
                        pr_duplicate: false,
                        base: base_branch.clone(),
                        remote: "origin".to_string(),
                    },
                },
            )?;
            return Ok(Some(RemoteIntegrationReport {
                branch: Some(branch),
                base_branch: Some(base_branch),
                pushed: false,
                pr_created: false,
                pr_duplicate: false,
                dry_run: true,
                confirmation_required: false,
                confirmation_granted: false,
                actions,
                pr_url: None,
                reason: Some("dry_run".to_string()),
                telemetry_path: Some(telemetry_path),
            }));
        }

        let confirmation_required = !options.auto_remote;
        let confirmation_granted = if options.auto_remote {
            true
        } else {
            confirmer.confirm_remote("Remote push and create PR? (y/n)")?
        };
        if !confirmation_granted {
            let telemetry_path = persist_remote_telemetry(
                root,
                &RemoteTelemetry {
                    remote: RemoteTelemetryData {
                        branch: branch.clone(),
                        dry_run_ok: true,
                        push_ok: false,
                        pr_created: false,
                        pr_duplicate: false,
                        base: base_branch.clone(),
                        remote: "origin".to_string(),
                    },
                },
            )?;
            return Ok(Some(RemoteIntegrationReport {
                branch: Some(branch),
                base_branch: Some(base_branch),
                pushed: false,
                pr_created: false,
                pr_duplicate: false,
                dry_run: false,
                confirmation_required,
                confirmation_granted: false,
                actions,
                pr_url: None,
                reason: Some("push_skipped".to_string()),
                telemetry_path: Some(telemetry_path),
            }));
        }

        PushController::push(root, &branch, false)?;
        actions.push(RemoteAction {
            r#type: "push".to_string(),
            target: format!("origin/{branch}"),
        });

        if let Some((number, url)) = PRManager::duplicate(root, &branch)? {
            let telemetry_path = persist_remote_telemetry(
                root,
                &RemoteTelemetry {
                    remote: RemoteTelemetryData {
                        branch: branch.clone(),
                        dry_run_ok: true,
                        push_ok: true,
                        pr_created: false,
                        pr_duplicate: true,
                        base: base_branch.clone(),
                        remote: "origin".to_string(),
                    },
                },
            )?;
            return Ok(Some(RemoteIntegrationReport {
                branch: Some(branch),
                base_branch: Some(base_branch),
                pushed: true,
                pr_created: false,
                pr_duplicate: true,
                dry_run: false,
                confirmation_required,
                confirmation_granted: true,
                actions,
                pr_url: url.or_else(|| number.map(|n| format!("PR#{n}"))),
                reason: Some("PRAlreadyExists".to_string()),
                telemetry_path: Some(telemetry_path),
            }));
        }

        let title = pr_title(attempts);
        let body = pr_body(git);
        let pr_url = PRManager::create(root, &branch, &base_branch, &title, &body)?;
        actions.push(RemoteAction {
            r#type: "pr_create".to_string(),
            target: branch.clone(),
        });
        let telemetry_path = persist_remote_telemetry(
            root,
            &RemoteTelemetry {
                remote: RemoteTelemetryData {
                    branch: branch.clone(),
                    dry_run_ok: true,
                    push_ok: true,
                    pr_created: true,
                    pr_duplicate: false,
                    base: base_branch.clone(),
                    remote: "origin".to_string(),
                },
            },
        )?;

        Ok(Some(RemoteIntegrationReport {
            branch: Some(branch),
            base_branch: Some(base_branch),
            pushed: true,
            pr_created: true,
            pr_duplicate: false,
            dry_run: false,
            confirmation_required,
            confirmation_granted: true,
            actions,
            pr_url: Some(pr_url),
            reason: None,
            telemetry_path: Some(telemetry_path),
        }))
    }
}

pub(super) struct RemoteGuard;

impl RemoteGuard {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn classify_git(args: &[&str]) -> CommandType {
        match args {
            ["push", "--dry-run", "origin", branch] if !is_protected_branch(branch) => {
                CommandType::SafeWrite
            }
            ["push", "origin", branch] if !is_protected_branch(branch) => CommandType::SafeWrite,
            _ => CommandType::Dangerous,
        }
    }

    pub(super) fn classify_gh(args: &[&str]) -> CommandType {
        match args {
            ["auth", "status"] | ["pr", "status"] => CommandType::SafeRead,
            ["pr", "view", ..] => CommandType::SafeRead,
            _ if args.starts_with(&["pr", "create"]) => CommandType::SafeWrite,
            _ => CommandType::Dangerous,
        }
    }
}

pub(super) struct BranchManager;

impl BranchManager {
    pub(super) fn create(root: &Path) -> Result<String, String> {
        let current = current_branch(root)?
            .ok_or_else(|| "RemoteBlocked: detached HEAD".to_string())?;
        if is_protected_branch(&current) {
            return Err("RemoteBlocked: protected branch".to_string());
        }
        let branch = format!("dbm/auto-fix/{}", current_timestamp_string()?);
        if !is_auto_fix_branch(&branch) {
            return Err("invalid auto-fix branch name".to_string());
        }
        GitExecutor::run_checked(root, &["checkout", "-b", &branch])?;
        Ok(branch)
    }
}

struct PushController;

impl PushController {
    fn push(root: &Path, branch: &str, dry_run: bool) -> Result<(), String> {
        let args = if dry_run {
            vec!["push", "--dry-run", "origin", branch]
        } else {
            vec!["push", "origin", branch]
        };
        let classified = RemoteGuard::classify_git(&args);
        if classified != CommandType::SafeWrite {
            return Err(format!("dangerous remote command rejected: git {}", args.join(" ")));
        }
        let output = git_command()
            .args(&args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git {}: {err}", args.join(" ")))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                "unknown git push error".to_string()
            } else {
                stderr
            })
        }
        .map_err(|err| {
            if dry_run {
                format!("RemoteDryRunFailed: {err}")
            } else {
                format!("PushFailed: {err}")
            }
        })
    }
}

struct PRManager;

impl PRManager {
    fn duplicate(root: &Path, branch: &str) -> Result<Option<(Option<u64>, Option<String>)>, String> {
        let args = ["pr", "view", branch, "--json", "number,url"];
        if RemoteGuard::classify_gh(&args) != CommandType::SafeRead {
            return Err("dangerous gh command rejected".to_string());
        }
        let output = Command::new(resolve_external_tool("gh"))
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run gh pr view: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| format!("failed to parse gh pr view output: {err}"))?;
        Ok(Some((
            value.get("number").and_then(|value| value.as_u64()),
            value.get("url")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
        )))
    }

    fn create(
        root: &Path,
        branch: &str,
        base_branch: &str,
        title: &str,
        body: &str,
    ) -> Result<String, String> {
        let args = [
            "pr",
            "create",
            "--base",
            base_branch,
            "--head",
            branch,
            "--title",
            title,
            "--body",
            body,
        ];
        if RemoteGuard::classify_gh(&args) != CommandType::SafeWrite {
            return Err("dangerous gh command rejected".to_string());
        }
        let output = Command::new(resolve_external_tool("gh"))
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run gh pr create: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "failed to create PR: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

pub(super) struct AuthValidator;

impl AuthValidator {
    pub(super) fn validate(root: &Path) -> Result<Option<String>, String> {
        let remote = git_command()
            .args(["remote", "get-url", "origin"])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to inspect origin remote: {err}"))?;
        if !remote.status.success() {
            return Ok(Some("RemoteBlocked: origin only".to_string()));
        }

        let args = ["auth", "status"];
        if RemoteGuard::classify_gh(&args) != CommandType::SafeRead {
            return Err("dangerous gh auth command rejected".to_string());
        }
        let output = Command::new(resolve_external_tool("gh"))
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run gh auth status: {err}"))?;
        if output.status.success() {
            Ok(None)
        } else {
            Ok(Some("RemoteAuthFailed".to_string()))
        }
    }
}

fn resolve_external_tool(name: &str) -> String {
    match name {
        "gh" => std::env::var("DBM_GH_BIN").unwrap_or_else(|_| "gh".to_string()),
        _ => name.to_string(),
    }
}

fn pr_title(attempts: &[ExecuteAttempt]) -> String {
    let summary = attempts
        .iter()
        .filter_map(|attempt| attempt.fix.as_ref())
        .last()
        .map(|fix| fix.content.clone())
        .unwrap_or_else(|| "deterministic update".to_string());
    format!("auto fix: {summary}")
}

fn pr_body(git: &GitIntegrationReport) -> String {
    let commit_hash = git.commit_id.as_deref().unwrap_or("unknown");
    format!(
        "- generated by design_cli\n- sandbox build verified\n- local commit hash: {commit_hash}"
    )
}

fn current_timestamp_string() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    Ok(format!(
        "{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}"
    ))
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn current_branch(root: &Path) -> Result<Option<String>, String> {
    let output = git_command()
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to inspect branch: {err}"))?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
    } else {
        Ok(None)
    }
}

fn index_is_clean(root: &Path) -> Result<bool, String> {
    let output = git_command()
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to inspect git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to inspect git status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn persist_remote_telemetry(root: &Path, telemetry: &RemoteTelemetry) -> Result<PathBuf, String> {
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir)
        .map_err(|err| format!("failed to create telemetry dir: {err}"))?;
    let telemetry_path = telemetry_dir.join("remote_integration.json");
    let body = serde_json::to_string_pretty(telemetry)
        .map_err(|err| format!("failed to serialize telemetry: {err}"))?;
    fs::write(&telemetry_path, body)
        .map_err(|err| format!("failed to persist telemetry: {err}"))?;
    Ok(telemetry_path)
}

fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
        || branch.starts_with("release/")
        || branch.starts_with("hotfix/")
        || branch.starts_with("production/")
}
