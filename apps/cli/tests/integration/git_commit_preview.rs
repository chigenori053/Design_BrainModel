use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::refactor::{GitCommitPreview, PromoteResult, generate_git_commit_preview};

use design_cli::test_support::with_current_dir;

fn temp_git_workspace(name: &str, branch: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_cli_git_preview_{name}_{unique}"));
    fs::create_dir_all(root.join("src")).expect("src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"git_preview\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(root.join("src/lib.rs"), "pub fn lib() {}\n").expect("lib");
    std::process::Command::new("git")
        .args(["init", "-b", branch])
        .current_dir(&root)
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "dbm@example.com"])
        .current_dir(&root)
        .output()
        .expect("git email");
    std::process::Command::new("git")
        .args(["config", "user.name", "DBM"])
        .current_dir(&root)
        .output()
        .expect("git name");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .expect("git commit");
    root
}

fn with_workspace_root<T>(root: &Path, f: impl FnOnce() -> T) -> T {
    with_current_dir(root, f)
}

fn promote_result(workspace_write: bool) -> PromoteResult {
    PromoteResult {
        confirmed: true,
        workspace_write,
        written_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
        cargo_check: "passed".to_string(),
        rollback_executed: false,
    }
}

#[test]
fn git_commit_preview_is_generated_for_workspace_write() {
    let root = temp_git_workspace("normal", "feature/test");
    let preview = with_workspace_root(&root, || {
        generate_git_commit_preview(&promote_result(true)).expect("git preview")
    });

    assert!(preview.commit_allowed);
    assert!(!preview.push);
    assert_eq!(
        preview,
        GitCommitPreview {
            branch: preview.branch.clone(),
            protected_branch: false,
            commit_allowed: true,
            commit_message: "dbm: remove adapter -> world dependency".to_string(),
            changed_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
            push: false,
        }
    );
}

#[test]
fn git_commit_preview_is_none_without_workspace_write() {
    let root = temp_git_workspace("none", "feature/test");
    let preview = with_workspace_root(&root, || {
        generate_git_commit_preview(&promote_result(false))
    });

    assert!(preview.is_none());
}

#[test]
fn git_commit_preview_uses_safe_branch_pattern() {
    let root = temp_git_workspace("branch", "main");
    let preview = with_workspace_root(&root, || {
        generate_git_commit_preview(&promote_result(true)).expect("git preview")
    });

    assert!(preview.branch.starts_with("dbm/auto-fix/"));
    assert!(preview.protected_branch);
    assert!(!preview.commit_allowed);
}
