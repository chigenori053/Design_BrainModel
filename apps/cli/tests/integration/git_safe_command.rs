use std::process::Command;

fn cli() -> &'static str {
    env!("CARGO_BIN_EXE_design_cli")
}

fn temp_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "dbm_git_safe_command_{name}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("apps/cli/src")).expect("mkdir");
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

fn temp_repo_with_upstream(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
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
    (root, remote)
}

fn create_ahead_commit(root: &std::path::Path, marker: &str) {
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        format!("fn main() {{ println!(\"{marker}\"); }}\n"),
    )
    .expect("write");
    run_git(root, &["add", "apps/cli/src/main.rs"]);
    run_git(root, &["commit", "-m", marker]);
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

fn dbm(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(cli())
        .args(args)
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
fn git_status_allowed() {
    let root = temp_repo("status");
    let output = dbm(&root, &["git", "status"]);
    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_status");
}

#[test]
fn git_diff_allowed() {
    let root = temp_repo("diff");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"x\"); }\n",
    )
    .expect("write");
    let output = dbm(&root, &["git", "diff", "--", "apps/cli/src/main.rs"]);
    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_diff");
    assert!(
        output["data"]["diff"]
            .as_str()
            .expect("diff")
            .contains("println!")
    );
}

#[test]
fn git_diff_cached_allowed() {
    let root = temp_repo("diff_cached");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"cached\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);

    let output = dbm(&root, &["git", "diff", "--cached"]);

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_diff");
    assert!(
        output["data"]["diff"]
            .as_str()
            .expect("diff")
            .contains("cached")
    );
}

#[test]
fn git_diff_staged_allowed() {
    let root = temp_repo("diff_staged");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"staged\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);

    let output = dbm(&root, &["git", "diff", "--staged"]);

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_diff");
    assert!(
        output["data"]["diff"]
            .as_str()
            .expect("diff")
            .contains("staged")
    );
}

#[test]
fn git_diff_cached_with_explicit_file_allowed() {
    let root = temp_repo("diff_cached_file");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"cached file\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);

    let output = dbm(
        &root,
        &["git", "diff", "--cached", "--", "apps/cli/src/main.rs"],
    );

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_diff");
    assert!(
        output["data"]["diff"]
            .as_str()
            .expect("diff")
            .contains("cached file")
    );
}

#[test]
fn git_diff_staged_with_explicit_file_allowed() {
    let root = temp_repo("diff_staged_file");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"staged file\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);

    let output = dbm(
        &root,
        &["git", "diff", "--staged", "--", "apps/cli/src/main.rs"],
    );

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_diff");
    assert!(
        output["data"]["diff"]
            .as_str()
            .expect("diff")
            .contains("staged file")
    );
}

#[test]
fn git_diff_unknown_option_rejected() {
    let root = temp_repo("diff_unknown_option");

    let output = dbm(&root, &["git", "diff", "--name-only"]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_diff");
    assert_eq!(output["reason"], "unsupported_diff_option");
}

#[test]
fn git_add_explicit_file_allowed() {
    let root = temp_repo("add");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"x\"); }\n",
    )
    .expect("write");
    let output = dbm(&root, &["git", "add", "apps/cli/src/main.rs"]);
    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_add");
}

#[test]
fn git_add_ambiguous_scope_rejected() {
    let root = temp_repo("add_dot");
    for forbidden in [
        ["git", "add", "."],
        ["git", "add", "-A"],
        ["git", "add", "--all"],
    ] {
        let output = dbm(&root, &forbidden);
        assert_eq!(output["status"], "rejected");
        assert_eq!(output["operation"], "git_add");
    }
}

#[test]
fn git_workspace_escape_rejected() {
    let root = temp_repo("escape");
    let outside = root.parent().expect("parent").join("outside.rs");
    std::fs::write(&outside, "fn outside() {}\n").expect("outside");
    let output = dbm(&root, &["git", "add", "../outside.rs"]);
    assert_eq!(output["status"], "rejected");
    assert_eq!(output["reason"], "workspace_escape_rejected");
}

#[test]
fn git_commit_requires_confirmation() {
    let root = temp_repo("commit");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"x\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    let output = dbm(&root, &["git", "commit", "-m", "update integration gate"]);
    assert_eq!(output["status"], "confirmation_required");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["data"]["message"], "update integration gate");
    assert_eq!(output["data"]["staged_files"][0], "apps/cli/src/main.rs");
    assert!(output["data"]["staged_checksum"].as_str().is_some());
    assert!(output["data"]["confirmation_token"].as_str().is_some());
    assert!(root.join(".dbm/pending_git_commit.json").exists());
}

#[test]
fn git_commit_empty_message_rejected() {
    let root = temp_repo("commit_empty_message");

    let output = dbm(&root, &["git", "commit", "-m", "   "]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["reason"], "empty_commit_message");
}

#[test]
fn git_commit_without_staged_files_rejected() {
    let root = temp_repo("commit_without_staged_files");

    let output = dbm(&root, &["git", "commit", "-m", "nothing staged"]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["reason"], "nothing_staged");
}

#[test]
fn git_commit_confirm_rejects_wrong_token() {
    let root = temp_repo("commit_wrong_token");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"wrong token\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    let preview = dbm(&root, &["git", "commit", "-m", "wrong token"]);
    assert_eq!(preview["status"], "confirmation_required");

    let output = dbm(&root, &["git", "commit", "--confirm", "confirm_wrong"]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["reason"], "confirmation_token_mismatch");
    assert!(root.join(".dbm/pending_git_commit.json").exists());
}

#[test]
fn git_commit_confirm_rejects_stale_staged_diff() {
    let root = temp_repo("commit_stale");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"first\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    let preview = dbm(&root, &["git", "commit", "-m", "stale commit"]);
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token")
        .to_string();
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"second\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);

    let output = dbm(&root, &["git", "commit", "--confirm", &token]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["reason"], "staged_diff_changed");
    assert!(!root.join(".dbm/pending_git_commit.json").exists());
}

#[test]
fn git_commit_confirm_executes_when_checksum_matches() {
    let root = temp_repo("commit_confirm_executes");
    std::fs::write(
        root.join("apps/cli/src/main.rs"),
        "fn main() { println!(\"confirmed\"); }\n",
    )
    .expect("write");
    run_git(&root, &["add", "apps/cli/src/main.rs"]);
    let preview = dbm(&root, &["git", "commit", "-m", "confirmed commit"]);
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token")
        .to_string();

    let output = dbm(&root, &["git", "commit", "--confirm", &token]);

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_commit");
    assert_eq!(output["data"]["message"], "confirmed commit");
    assert!(!root.join(".dbm/pending_git_commit.json").exists());
    let log = Command::new("git")
        .args(["log", "-1", "--pretty=%s"])
        .current_dir(&root)
        .output()
        .expect("git log");
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "confirmed commit"
    );
}

#[test]
fn git_push_dry_run_allowed() {
    let (root, _remote) = temp_repo_with_upstream("push_dry_run");

    let dry_run = dbm(&root, &["git", "push", "--dry-run"]);
    assert_eq!(dry_run["status"], "ok");
    assert_eq!(dry_run["operation"], "git_push_dry_run");
}

#[test]
fn git_push_preview_requires_confirmation() {
    let (root, _remote) = temp_repo_with_upstream("push_preview");
    create_ahead_commit(&root, "push preview");

    let output = dbm(&root, &["git", "push"]);

    assert_eq!(output["status"], "confirmation_required");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["data"]["remote"], "origin");
    assert_eq!(output["data"]["ahead_count"], 1);
    assert!(output["data"]["confirmation_token"].as_str().is_some());
    assert!(root.join(".dbm/pending_git_push.json").exists());
}

#[test]
fn git_push_confirm_rejects_wrong_token() {
    let (root, _remote) = temp_repo_with_upstream("push_wrong_token");
    create_ahead_commit(&root, "push wrong token");
    let preview = dbm(&root, &["git", "push"]);
    assert_eq!(preview["status"], "confirmation_required");

    let output = dbm(&root, &["git", "push", "--confirm", "confirm_wrong"]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["reason"], "token_mismatch");
    assert!(root.join(".dbm/pending_git_push.json").exists());
}

#[test]
fn git_push_confirm_rejects_head_changed() {
    let (root, _remote) = temp_repo_with_upstream("push_head_changed");
    create_ahead_commit(&root, "push first");
    let preview = dbm(&root, &["git", "push"]);
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token")
        .to_string();
    create_ahead_commit(&root, "push second");

    let output = dbm(&root, &["git", "push", "--confirm", &token]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["reason"], "head_changed");
    assert!(!root.join(".dbm/pending_git_push.json").exists());
}

#[test]
fn git_push_confirm_executes_when_snapshot_matches() {
    let (root, remote) = temp_repo_with_upstream("push_confirm");
    create_ahead_commit(&root, "push confirmed");
    let preview = dbm(&root, &["git", "push"]);
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token")
        .to_string();

    let output = dbm(&root, &["git", "push", "--confirm", &token]);

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_push");
    assert!(!root.join(".dbm/pending_git_push.json").exists());
    let local = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .expect("local head");
    let remote = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&remote)
        .output()
        .expect("remote head");
    assert_eq!(
        String::from_utf8_lossy(&local.stdout).trim(),
        String::from_utf8_lossy(&remote.stdout).trim()
    );
}

#[test]
fn git_push_force_rejected() {
    let root = temp_repo("push_force");

    let force = dbm(&root, &["git", "push", "--force"]);
    assert_eq!(force["status"], "rejected");
    assert_eq!(force["operation"], "git_push");
    assert_eq!(force["reason"], "force_push_rejected");
}

#[test]
fn git_push_explicit_remote_branch_rejected() {
    let root = temp_repo("push_explicit");

    let output = dbm(&root, &["git", "push", "origin", "main"]);

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["reason"], "explicit_remote_branch_not_supported");
}
