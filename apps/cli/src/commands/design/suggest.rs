use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{detect_issues_for, load_versions, resolve_root};
use unified_design_ir::{FixInput, apply_next_fix, is_converged};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("suggest", execute)
}

/// /design suggest [path]
///
/// 次に適用される Fix を表示する（実際には適用しない）。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));

    let (current, history) = load_versions(&root).map_err(CommandError::ExecutionError)?;

    let issues = detect_issues_for(&history, &current).map_err(CommandError::ExecutionError)?;

    if is_converged(&issues) {
        return Ok(Output::text(
            "Status: Converged\nNo fixes needed. Design is consistent.".to_string(),
        ));
    }

    // Run apply_next_fix (dry-run: don't save)
    let fix_result = apply_next_fix(FixInput {
        history,
        current,
        issues: issues.issues.clone(),
    });

    let mut report = String::from("Suggestion (not applied):\n\n");

    if let Some(fix) = &fix_result.applied {
        report.push_str(&format!(
            "  Action : {:?}\n  Path   : {}\n",
            fix.action,
            fix.path.segments.join(".")
        ));
    } else {
        report.push_str("  (no applicable fix)\n");
    }

    if fix_result.report.success {
        // Show what the design would look like after the fix
        report.push_str("\nAfter fix:\n");
        report.push_str(&super::design_to_yaml(&fix_result.next_version.design));
    } else {
        report.push_str(&format!(
            "\nFix would fail: {:?}\n",
            fix_result.report.reason
        ));
    }

    report.push_str("\nRemaining Issues:\n");
    report.push_str(&super::format_summary(&issues));

    Ok(Output::text(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;
    use std::fs;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm_suggest_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn suggest_converged_design() {
        let dir = tmp_dir("converged");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: build application\n  environment: local\nmetadata:\n  tags: []\n",
        ).unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Converged") || out.message.contains("Suggestion"));
    }
}
