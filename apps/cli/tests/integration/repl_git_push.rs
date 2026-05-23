use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

fn run_git(root: &Path, args: &[&str]) {
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

fn temp_repo(name: &str) -> PathBuf {
    let root = tempfile::tempdir().expect("tempdir").keep();
    let root = root.join(name);
    std::fs::create_dir_all(&root).expect("repo dir");
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "dbm@example.test"]);
    run_git(&root, &["config", "user.name", "DBM Test"]);
    std::fs::write(root.join("README.md"), "initial\n").expect("readme");
    run_git(&root, &["add", "README.md"]);
    run_git(&root, &["commit", "-m", "initial"]);
    root
}

fn temp_repo_with_upstream(name: &str) -> (PathBuf, PathBuf) {
    let root = temp_repo(name);
    let remote = root.with_extension("remote.git");
    run_git(
        root.parent().expect("parent"),
        &["init", "--bare", remote.to_str().expect("remote")],
    );
    run_git(
        &root,
        &["remote", "add", "origin", remote.to_str().expect("remote")],
    );
    run_git(&root, &["push", "-u", "origin", "HEAD"]);
    (root, remote)
}

fn create_ahead_commit(root: &Path, message: &str) {
    let path = root.join("README.md");
    let mut body = std::fs::read_to_string(&path).expect("readme");
    body.push_str(message);
    body.push('\n');
    std::fs::write(path, body).expect("write readme");
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", message]);
}

fn run_repl(root: &Path, input: &str) -> String {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .arg("repl")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write repl input");
    let output = child.wait_with_output().expect("wait repl");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
    stdout
}

fn first_json(stdout: &str) -> Value {
    stdout
        .lines()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap_or_else(|| panic!("no json line in stdout:\n{stdout}"))
}

#[test]
fn repl_git_push_dry_run_allowed() {
    let (root, _remote) = temp_repo_with_upstream("repl_push_dry_run");

    let output = first_json(&run_repl(&root, "git push --dry-run\n/exit\n"));

    assert_eq!(output["status"], "ok");
    assert_eq!(output["operation"], "git_push_dry_run");
}

#[test]
fn repl_git_push_requires_confirmation() {
    let (root, _remote) = temp_repo_with_upstream("repl_push_preview");
    create_ahead_commit(&root, "push preview");

    let output = first_json(&run_repl(&root, "git push\n/exit\n"));

    assert_eq!(output["status"], "confirmation_required");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["data"]["remote"], "origin");
    assert_eq!(output["data"]["ahead_count"], 1);
    assert!(output["data"]["confirmation_token"].as_str().is_some());
    assert!(root.join(".dbm/pending_git_push.json").exists());
}

#[test]
fn repl_git_push_confirm_rejects_wrong_token() {
    let (root, _remote) = temp_repo_with_upstream("repl_push_wrong_token");
    create_ahead_commit(&root, "push wrong token");
    let preview = first_json(&run_repl(&root, "git push\n/exit\n"));
    assert_eq!(preview["status"], "confirmation_required");

    let output = first_json(&run_repl(
        &root,
        "git push --confirm confirm_wrong\n/exit\n",
    ));

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["reason"], "token_mismatch");
    assert!(root.join(".dbm/pending_git_push.json").exists());
}

#[test]
fn repl_git_push_confirm_executes_when_snapshot_matches() {
    let (root, remote) = temp_repo_with_upstream("repl_push_confirm");
    create_ahead_commit(&root, "push confirmed");
    let preview = first_json(&run_repl(&root, "git push\n/exit\n"));
    let token = preview["data"]["confirmation_token"]
        .as_str()
        .expect("token");

    let output = first_json(&run_repl(
        &root,
        &format!("git push --confirm {token}\n/exit\n"),
    ));

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
fn repl_git_push_force_rejected() {
    let root = temp_repo("repl_push_force");

    let output = first_json(&run_repl(&root, "git push --force\n/exit\n"));

    assert_eq!(output["status"], "rejected");
    assert_eq!(output["operation"], "git_push");
    assert_eq!(output["reason"], "force_push_rejected");
}
