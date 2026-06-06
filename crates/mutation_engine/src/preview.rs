use std::fs;
use std::path::Path;

use crate::error::MutationError;
use crate::model::{FileDiff, MutationPlan, MutationPreview, PatchOperation};
use crate::validator::requires_confirmation;

pub fn preview(workspace: &Path, plan: &MutationPlan) -> Result<MutationPreview, MutationError> {
    let mut files = Vec::new();
    for patch in &plan.patches {
        match patch {
            PatchOperation::Write { path, content } => {
                let old = fs::read_to_string(workspace.join(path)).unwrap_or_default();
                files.push(FileDiff {
                    path: path.clone(),
                    unified_diff: unified_diff(path, &old, content),
                });
            }
            PatchOperation::Delete { path } => {
                let old = fs::read_to_string(workspace.join(path)).unwrap_or_default();
                files.push(FileDiff {
                    path: path.clone(),
                    unified_diff: unified_diff(path, &old, ""),
                });
            }
            PatchOperation::Move { from, to } => {
                let old = fs::read_to_string(workspace.join(from)).unwrap_or_default();
                files.push(FileDiff {
                    path: from.clone(),
                    unified_diff: unified_diff(from, &old, ""),
                });
                files.push(FileDiff {
                    path: to.clone(),
                    unified_diff: unified_diff(to, "", &old),
                });
            }
        }
    }
    Ok(MutationPreview {
        mutation_id: plan.id.clone(),
        files,
        confirmation_required: requires_confirmation(plan),
    })
}

fn unified_diff(path: &Path, old: &str, new: &str) -> String {
    let mut diff = format!("--- a/{}\n+++ b/{}\n", path.display(), path.display());
    let old_lines = old.lines().collect::<Vec<_>>();
    let new_lines = new.lines().collect::<Vec<_>>();
    let common_prefix = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(left, right)| left == right)
        .count();
    let common_suffix = old_lines[common_prefix..]
        .iter()
        .rev()
        .zip(new_lines[common_prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_end = old_lines.len().saturating_sub(common_suffix);
    let new_end = new_lines.len().saturating_sub(common_suffix);
    diff.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        common_prefix + 1,
        old_end.saturating_sub(common_prefix),
        common_prefix + 1,
        new_end.saturating_sub(common_prefix)
    ));
    for line in &old_lines[common_prefix..old_end] {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in &new_lines[common_prefix..new_end] {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}
