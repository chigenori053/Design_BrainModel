use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn cli() -> &'static str {
    env!("CARGO_BIN_EXE_design_cli")
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "dbm_github_safe_command_{name}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

fn fake_gh_dir() -> std::path::PathBuf {
    let dir = temp_dir("fake_gh");
    let gh = dir.join("gh");
    std::fs::write(
        &gh,
        r#"#!/bin/sh
case "$*" in
  "auth status") exit 0 ;;
  "repo view") echo "owner/repo"; exit 0 ;;
  "repo view --json nameWithOwner") echo '{"nameWithOwner":"owner/repo"}'; exit 0 ;;
  "pr status") echo "ok"; exit 0 ;;
  "pr view 1") echo "PR #1"; exit 0 ;;
  "pr diff 1") echo "diff --git a/file b/file"; exit 0 ;;
  "issue view 1") echo "Issue #1"; exit 0 ;;
  "issue list") echo "Issue list"; exit 0 ;;
  "pr create --title test --body body --base main") echo "https://github.com/owner/repo/pull/1"; exit 0 ;;
  *) echo "unsupported fake gh: $*" >&2; exit 1 ;;
esac
"#,
    )
    .expect("write fake gh");
    let mut permissions = std::fs::metadata(&gh).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&gh, permissions).expect("chmod");
    dir
}

fn temp_repo(name: &str) -> std::path::PathBuf {
    let root = temp_dir(name);
    std::fs::create_dir_all(root.join("apps/cli/src")).expect("mkdir src");
    std::fs::write(root.join("apps/cli/src/main.rs"), "fn main() {}\n").expect("write");
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.name", "DBM CLI Test"]);
    run_git(
        &root,
        &["config", "user.email", "dbm-cli-test@example.invalid"],
    );
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    run_git(&root, &["commit", "-m", "initial"]);
    root
}

fn temp_pr_repo(name: &str) -> std::path::PathBuf {
    let root = temp_repo(name);
    let remote = root.with_extension("remote.git");
    run_git(
        remote.parent().expect("parent"),
        &["init", "--bare", remote.to_str().expect("remote")],
    );
    run_git(
        &root,
        &["remote", "add", "origin", remote.to_str().expect("remote")],
    );
    run_git(&root, &["push", "-u", "origin", "HEAD"]);
    run_git(&root, &["checkout", "-b", "feature/github-safe"]);
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"github\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    run_git(&root, &["commit", "-m", "feature"]);
    run_git(&root, &["push", "-u", "origin", "HEAD"]);
    root
}

fn run_git(root: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {:?}\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn dbm(root: &std::path::Path, fake_gh: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let path = format!(
        "{}:{}",
        fake_gh.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(cli())
        .args(args)
        .env("PATH", path)
        .current_dir(root)
        .output()
        .expect("dbm");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "invalid json: {err}\nstatus={}\nstdout={}\nstderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn github_readonly_commands_are_allowed() {
    let root = temp_repo("readonly");
    let fake_gh = fake_gh_dir();

    for (args, operation) in [
        (&["github", "auth", "status"][..], "github_auth_status"),
        (&["github", "repo", "view"][..], "github_repo_view"),
        (&["github", "pr", "status"][..], "github_pr_status"),
        (&["github", "pr", "view", "1"][..], "github_pr_view"),
        (&["github", "pr", "diff", "1"][..], "github_pr_diff"),
        (&["github", "issue", "view", "1"][..], "github_issue_view"),
        (&["github", "issue", "list"][..], "github_issue_list"),
        (&["gh", "pr", "status"][..], "github_pr_status"),
    ] {
        let output = dbm(&root, &fake_gh, args);
        assert_eq!(output["status"], "ok", "{args:?}");
        assert_eq!(output["operation"], operation, "{args:?}");
    }
}

#[test]
fn github_pr_create_requires_confirmation() {
    let root = temp_pr_repo("pr_preview");
    let fake_gh = fake_gh_dir();

    let output = dbm(
        &root,
        &fake_gh,
        &[
            "github", "pr", "create", "--title", "test", "--body", "body",
        ],
    );

    assert_eq!(output["status"], "confirmation_required");
    assert_eq!(output["operation"], "github_pr_create");
    assert_eq!(output["data"]["title"], "test");
    assert_eq!(output["data"]["body"], "body");
    assert_eq!(output["data"]["base"], "main");
    assert_eq!(output["data"]["head"], "feature/github-safe");
    assert!(output["data"]["confirmation_token"].as_str().is_some());
    assert!(root.join(".dbm/pending_github_pr_create.json").exists());
}

#[test]
fn github_pr_create_confirm_rejects_wrong_token() {
    let root = temp_pr_repo("pr_wrong_token");
    let fake_gh = fake_gh_dir();
    let preview = dbm(
        &root,
        &fake_gh,
        &[
            "github", "pr", "create", "--title", "test", "--body", "body",
        ],
    );
    assert_eq!(preview["status"], "confirmation_required");

    let output = dbm(
        &root,
        &fake_gh,
        &["github", "pr", "create", "--confirm", "confirm_wrong"],
    );

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "github_pr_create");
    assert_eq!(output["reason"], "token_mismatch");
    assert!(root.join(".dbm/pending_github_pr_create.json").exists());
}

#[test]
fn github_pr_create_confirm_executes_when_snapshot_matches() {
    let root = temp_pr_repo("pr_confirm");
    let fake_gh = fake_gh_dir();
    let preview = dbm(
        &root,
        &fake_gh,
        &[
            "github", "pr", "create", "--title", "test", "--body", "body",
        ],
    );
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token")
        .to_string();

    let output = dbm(
        &root,
        &fake_gh,
        &["github", "pr", "create", "--confirm", &token],
    );

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "github_pr_create");
    assert_eq!(
        output["data"]["url"],
        "https://github.com/owner/repo/pull/1"
    );
    assert!(!root.join(".dbm/pending_github_pr_create.json").exists());
}

#[test]
fn github_destructive_commands_are_rejected() {
    let root = temp_repo("destructive");
    let fake_gh = fake_gh_dir();

    for args in [
        &["github", "pr", "merge", "1"][..],
        &["github", "pr", "close", "1"][..],
        &["github", "issue", "close", "1"][..],
        &["github", "release", "create", "v1"][..],
        &["github", "secret", "set", "TOKEN"][..],
        &["github", "workflow", "run", "ci.yml"][..],
    ] {
        let output = dbm(&root, &fake_gh, args);
        assert_eq!(output["status"], "rejected", "{args:?}");
        assert_eq!(output["operation"], "github", "{args:?}");
        assert_eq!(
            output["reason"], "destructive_github_command_rejected",
            "{args:?}"
        );
    }
}
