use std::path::{Path, PathBuf};

use super::executor::{git_lines, run_git};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyTreePolicy {
    pub ignored_paths: Vec<PathBuf>,
}

impl Default for DirtyTreePolicy {
    fn default() -> Self {
        Self {
            ignored_paths: vec![PathBuf::from(".dbm/"), PathBuf::from("target/tmp/")],
        }
    }
}

impl DirtyTreePolicy {
    pub fn is_ignored(&self, path: &Path) -> bool {
        self.ignored_paths
            .iter()
            .any(|ignored| path.starts_with(ignored))
    }

    pub fn is_allowed(&self, path: &Path, allowed_target: Option<&Path>) -> bool {
        allowed_target.is_some_and(|target| path == target) || self.is_ignored(path)
    }
}

pub fn reject_dirty_worktree_except(
    root: &Path,
    policy: &DirtyTreePolicy,
    allowed_target: Option<&Path>,
) -> Result<(), String> {
    let rejected = dirty_paths(root)?
        .into_iter()
        .filter(|path| !policy.is_allowed(path, allowed_target))
        .collect::<Vec<_>>();
    if rejected.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "working tree dirty: {}",
            rejected
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

pub fn reject_unstaged_worktree(root: &Path, policy: &DirtyTreePolicy) -> Result<(), String> {
    let rejected = git_lines(root, &["diff", "--name-only"])?
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| !policy.is_ignored(path))
        .collect::<Vec<_>>();
    if rejected.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "working tree dirty: {}",
            rejected
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

pub fn dirty_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(run_git(root, &["status", "--porcelain"])?
        .lines()
        .filter_map(porcelain_path)
        .map(PathBuf::from)
        .collect())
}

fn porcelain_path(line: &str) -> Option<&str> {
    if line.len() < 4 {
        return None;
    }
    line[3..].rsplit(" -> ").next().map(str::trim)
}
