use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{
    detect_issues_for, load_versions, resolve_root, save_baseline, save_design_doc,
    save_version_snapshot,
};
use unified_design_ir::{FixInput, apply_next_fix, is_converged};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("step", execute)
}

/// /design step [path]
///
/// 1 iterationのみ実行する（ユーザーが介入できる手動収束）。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));

    let (current, history) = load_versions(&root).map_err(|e| CommandError::ExecutionError(e))?;

    let issues =
        detect_issues_for(&history, &current).map_err(|e| CommandError::ExecutionError(e))?;

    if is_converged(&issues) {
        return Ok(Output::text(
            "Status: Converged\nNo issues detected. Design is already consistent.".to_string(),
        ));
    }

    let fix_result = apply_next_fix(FixInput {
        history,
        current,
        issues: issues.issues,
    });

    if !fix_result.report.success {
        return Ok(Output::text(format!(
            "Fix failed: {:?}\nDesign unchanged.",
            fix_result.report.reason
        )));
    }

    let next = &fix_result.next_version;

    save_design_doc(&root, &next.design).map_err(|e| CommandError::ExecutionError(e))?;
    save_baseline(&root, next).map_err(|e| CommandError::ExecutionError(e))?;
    save_version_snapshot(&root, next).map_err(|e| CommandError::ExecutionError(e))?;

    let fix_str = fix_result
        .applied
        .as_ref()
        .map(|f| format!("{:?} on {}", f.action, f.path.segments.join(".")))
        .unwrap_or_else(|| "(none)".to_string());

    Ok(Output::text(format!(
        "Applied Fix:\n  {}\n\nVersion: v{}  (run `design analyze` to see remaining issues)",
        fix_str, next.id.seq
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;
    use std::fs;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm_step_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn step_on_converged_design_reports_no_issues() {
        let dir = tmp_dir("converged");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: build application\n  environment: local\nmetadata:\n  tags: []\n",
        ).unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Converged"));
    }
}
