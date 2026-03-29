use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{detect_issues_for, format_issue_list, format_summary, load_versions, resolve_root};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("analyze", execute)
}

/// /design analyze [path]
///
/// 現在の設計の Issue 一覧と収束状態を表示する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));

    let (current, history) = load_versions(&root).map_err(|e| CommandError::ExecutionError(e))?;

    let issues =
        detect_issues_for(&history, &current).map_err(|e| CommandError::ExecutionError(e))?;

    let converged = issues.summary.critical == 0 && issues.summary.high == 0;
    let status = if converged {
        "Converged"
    } else {
        "In Progress"
    };

    let mut report = format!(
        "Stage: {:?}\nStatus: {}\n\nIssues:\n",
        current.stage, status
    );
    report.push_str(&format_issue_list(&issues));
    report.push('\n');
    report.push_str(&format_summary(&issues));

    Ok(Output::text(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;
    use std::fs;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm_analyze_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn analyze_shows_stage_and_status() {
        let dir = tmp_dir("stage_status");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: build app\n  environment: local\nmetadata:\n  tags: []\n",
        ).unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Stage:"));
        assert!(out.message.contains("Status:"));
        assert!(out.message.contains("Issues:"));
    }
}
