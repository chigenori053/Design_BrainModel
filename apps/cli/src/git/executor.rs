use std::path::Path;
use std::process::Command;

use super::commands::{DBM_FIXED_COMMIT_MESSAGE, GitCommand};

pub fn execute_read(root: &Path, command: &GitCommand) -> Result<String, String> {
    match command {
        GitCommand::Status => run_git(root, &["status", "--porcelain"]),
        GitCommand::Diff => run_git(root, &["diff"]),
        GitCommand::Log => run_git(root, &["log", "--oneline"]),
        _ => Err("not a safe read git command".to_string()),
    }
}

pub fn add_file(root: &Path, path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("add")
        .arg("--")
        .arg(path)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git add: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn commit_fixed(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["-c", "user.name=DBM CLI"])
        .args(["-c", "user.email=dbm-cli@example.invalid"])
        .arg("commit")
        .arg("-m")
        .arg(DBM_FIXED_COMMIT_MESSAGE)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git commit: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    run_git(root, &["rev-parse", "--short", "HEAD"]).map(|hash| hash.trim().to_string())
}

pub fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn git_lines(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(run_git(root, args)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}
