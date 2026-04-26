use std::path::Path;

use crate::refactor::{PreviewDiff, RefactorPlan};

#[deprecated(note = "Legacy preview path disabled; use deterministic execute_refactor")]
pub fn preview_diff_for_plan(root: &Path, plan: &RefactorPlan) -> Result<PreviewDiff, String> {
    let _ = (root, plan);
    panic!("Legacy path disabled");
}
