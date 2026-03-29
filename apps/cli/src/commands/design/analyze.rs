use std::path::Path;

use serde::{Deserialize, Serialize};
use unified_design_ir::{Issue as DesignIssue, IssueSummary as DesignIssueSummary, Stage};

use crate::command::{CommandError, Output, SubCommandHandler};
use crate::renderer::render_dbm_analysis_result;
use crate::session::AgentSession;

use super::{detect_issues_for, load_versions, resolve_root};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub input: String,
    pub stage: Stage,
    pub converged: bool,
    pub status: AnalysisStatus,
    pub summary: DesignIssueSummary,
    pub issues: Vec<DesignIssue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisStatus {
    Converged,
    InProgress,
}

impl AnalysisStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "Converged",
            Self::InProgress => "In Progress",
        }
    }
}

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("analyze", execute)
}

/// /design analyze [path]
///
/// 現在の設計の Issue 一覧と収束状態を表示する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));
    let result = analyze(&root).map_err(CommandError::ExecutionError)?;
    Ok(Output::text(render_dbm_analysis_result(&result)))
}

pub fn analyze(root: &Path) -> Result<AnalysisResult, String> {
    let (current, history) = load_versions(root)?;
    let issues = detect_issues_for(&history, &current)?;

    let converged = issues.summary.critical == 0 && issues.summary.high == 0;
    let status = if converged {
        AnalysisStatus::Converged
    } else {
        AnalysisStatus::InProgress
    };

    Ok(AnalysisResult {
        input: root.join("design.md").display().to_string(),
        stage: current.stage,
        converged,
        status,
        summary: issues.summary,
        issues: issues.issues,
    })
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

    #[test]
    fn analyze_returns_pure_deterministic_data() {
        let dir = tmp_dir("pure_data");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: build app\n  environment: local\nmetadata:\n  tags: []\n",
        )
        .unwrap();

        let first = analyze(&dir).unwrap();
        let second = analyze(&dir).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.status, AnalysisStatus::InProgress);
        assert!(serde_json::to_string(&first).is_ok());
    }
}
