use super::git_integration::{GitExecutor, git_command, is_auto_fix_branch, is_feature_branch};
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
    pub dry_run: bool,
    pub confirmation_required: bool,
    pub confirmation_granted: bool,
    pub actions: Vec<RemoteAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

        let confirmation_required = !options.auto_remote;
        let confirmation_granted = if options.auto_remote {
            true
        } else {
            confirmer.confirm_remote("Push & create PR? (y/n)")?
        };
        if !confirmation_granted {
            return Ok(Some(RemoteIntegrationReport {
                branch: None,
                base_branch: None,
                pushed: false,
                pr_created: false,
                dry_run: options.dry_run,
                confirmation_required,
                confirmation_granted: false,
                actions: Vec::new(),
                pr_url: None,
                reason: Some("remote_declined".to_string()),
            }));
        }

        if let Some(reason) = AuthValidator::validate(root)? {
            return Ok(Some(RemoteIntegrationReport {
                branch: None,
                base_branch: None,
                pushed: false,
                pr_created: false,
                dry_run: options.dry_run,
                confirmation_required,
                confirmation_granted: true,
                actions: Vec::new(),
                pr_url: None,
                reason: Some(reason),
            }));
        }

        let branch = BranchManager::create(root)?;
        let base_branch = detect_base_branch(root)?;
        let mut actions = vec![RemoteAction {
            r#type: "branch".to_string(),
            target: branch.clone(),
        }];

        PushController::push(root, &branch, options.dry_run)?;
        actions.push(RemoteAction {
            r#type: if options.dry_run {
                "push_dry_run".to_string()
            } else {
                "push".to_string()
            },
            target: format!("origin/{branch}"),
        });

        if options.dry_run {
            return Ok(Some(RemoteIntegrationReport {
                branch: Some(branch),
                base_branch: Some(base_branch),
                pushed: false,
                pr_created: false,
                dry_run: true,
                confirmation_required,
                confirmation_granted: true,
                actions,
                pr_url: None,
                reason: Some("dry_run".to_string()),
            }));
        }

        let create_pr = if options.auto_remote {
            true
        } else {
            confirmer.confirm_remote("Create PR? (y/n)")?
        };
        if !create_pr {
            return Ok(Some(RemoteIntegrationReport {
                branch: Some(branch),
                base_branch: Some(base_branch),
                pushed: true,
                pr_created: false,
                dry_run: false,
                confirmation_required,
                confirmation_granted: true,
                actions,
                pr_url: None,
                reason: Some("pr_creation_declined".to_string()),
            }));
        }

        let title = pr_title(attempts);
        let body = pr_body(attempts, git);
        let pr_url = PRManager::create(root, &branch, &base_branch, &title, &body)?;
        actions.push(RemoteAction {
            r#type: "pr_create".to_string(),
            target: branch.clone(),
        });

        Ok(Some(RemoteIntegrationReport {
            branch: Some(branch),
            base_branch: Some(base_branch),
            pushed: true,
            pr_created: true,
            dry_run: false,
            confirmation_required,
            confirmation_granted: true,
            actions,
            pr_url: Some(pr_url),
            reason: None,
        }))
    }
}

pub(super) struct RemoteGuard;

impl RemoteGuard {
    pub(super) fn classify_git(args: &[&str]) -> CommandType {
        match args {
            ["push", "--dry-run", "origin", branch] if is_feature_branch(branch) => {
                CommandType::RemoteWrite
            }
            ["push", "origin", branch] if is_feature_branch(branch) => CommandType::RemoteWrite,
            ["push", ..] | ["rebase", ..] => CommandType::Dangerous,
            _ => CommandType::Dangerous,
        }
    }

    pub(super) fn classify_gh(args: &[&str]) -> CommandType {
        match args {
            ["auth", "status"] | ["pr", "status"] | ["pr", "view"] => CommandType::SafeRead,
            ["repo", "delete", ..] | ["api", ..] => CommandType::Dangerous,
            _ if args.starts_with(&["pr", "create"]) => CommandType::RemoteWrite,
            _ => CommandType::Dangerous,
        }
    }
}

pub(super) struct BranchManager;

impl BranchManager {
    pub(super) fn create(root: &Path) -> Result<String, String> {
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
        match RemoteGuard::classify_git(&args) {
            CommandType::RemoteWrite => {}
            _ => {
                return Err(format!(
                    "dangerous remote command rejected: git {}",
                    args.join(" ")
                ));
            }
        }
        let output = git_command()
            .args(&args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run git {}: {err}", args.join(" ")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

struct PRManager;

impl PRManager {
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
        match RemoteGuard::classify_gh(&args) {
            CommandType::RemoteWrite => {}
            _ => return Err("dangerous gh command rejected".to_string()),
        }
        let output = Command::new(resolve_external_tool("gh"))
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run gh pr create: {err}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if url.is_empty() {
            return Err("gh pr create returned no PR url".to_string());
        }
        Ok(url)
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
            return Ok(Some("origin_remote_not_configured".to_string()));
        }

        let args = ["auth", "status"];
        match RemoteGuard::classify_gh(&args) {
            CommandType::SafeRead => {}
            _ => return Err("dangerous gh auth command rejected".to_string()),
        }
        let output = Command::new(resolve_external_tool("gh"))
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to run gh auth status: {err}"))?;
        if output.status.success() {
            Ok(None)
        } else {
            Ok(Some("github_auth_invalid".to_string()))
        }
    }
}

fn resolve_external_tool(name: &str) -> String {
    match name {
        "gh" => std::env::var("DBM_GH_BIN").unwrap_or_else(|_| "gh".to_string()),
        _ => name.to_string(),
    }
}

fn detect_base_branch(root: &Path) -> Result<String, String> {
    for candidate in ["main", "master"] {
        let output = git_command()
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{candidate}"),
            ])
            .current_dir(root)
            .output()
            .map_err(|err| format!("failed to inspect base branch {candidate}: {err}"))?;
        if output.status.success() {
            return Ok(candidate.to_string());
        }
    }
    Err("base_branch_not_found".to_string())
}

fn pr_title(attempts: &[ExecuteAttempt]) -> String {
    let summary = attempts
        .iter()
        .filter_map(|attempt| attempt.fix.as_ref())
        .last()
        .map(|fix| fix.content.clone())
        .unwrap_or_else(|| "apply autonomous fix".to_string());
    format!("auto fix: {summary}")
}

fn pr_body(attempts: &[ExecuteAttempt], git: &GitIntegrationReport) -> String {
    let fix_chain = attempts
        .iter()
        .filter_map(|attempt| attempt.debug.as_ref())
        .map(|debug| normalize_fix_chain_step(&debug.primary.action))
        .collect::<Vec<_>>();
    let confidence = attempts
        .iter()
        .filter_map(|attempt| attempt.debug.as_ref())
        .map(|debug| debug.confidence)
        .fold(0.0_f32, f32::max);
    let confidence_label = if confidence >= 0.85 {
        "high"
    } else if confidence >= 0.65 {
        "medium"
    } else {
        "low"
    };
    let changes = if git.changed_files.is_empty() {
        "- None".to_string()
    } else {
        git.changed_files
            .iter()
            .map(|file| format!("- {file} updated"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "## Auto Fix Report\n\n- Fix chain: {}\n- Attempts: {}\n- Confidence: {}\n\n### Changes\n{}\n\n### Safety\n- Single file change\n- PreCommitValidator passed",
        if fix_chain.is_empty() {
            "unknown".to_string()
        } else {
            fix_chain.join(" -> ")
        },
        attempts
            .iter()
            .map(|attempt| attempt.attempt)
            .max()
            .unwrap_or(0),
        confidence_label,
        changes
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
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}
