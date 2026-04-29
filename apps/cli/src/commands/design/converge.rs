use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{
    load_versions, resolve_root, save_baseline, save_design_doc, save_version_snapshot, stage_str,
};
use unified_design_ir::{ConvergenceInput, ConvergenceStatus, converge};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("converge", execute)
}

/// /design converge [path]
///
/// design.md を読み込み、収束エンジンを実行して更新する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));

    let (initial, history) = load_versions(&root).map_err(CommandError::ExecutionError)?;

    let result = converge(ConvergenceInput { initial, history });

    // Persist results
    save_design_doc(&root, &result.final_version.design).map_err(CommandError::ExecutionError)?;
    save_baseline(&root, &result.final_version).map_err(CommandError::ExecutionError)?;
    save_version_snapshot(&root, &result.final_version).map_err(CommandError::ExecutionError)?;

    // Build report
    let status_str = match &result.status {
        ConvergenceStatus::Converged => "Converged ✓",
        ConvergenceStatus::MaxIterationsReached => "MaxIterationsReached",
        ConvergenceStatus::Deadlock => "Deadlock",
        ConvergenceStatus::Failed => "Failed",
    };

    let mut report = format!(
        "Iterations: {}\nStatus: {}\n",
        result.iterations, status_str
    );

    if !result.trace.is_empty() {
        report.push_str("\nTrace:\n");
        for (i, trace) in result.trace.iter().enumerate() {
            let fix_str = trace
                .applied_fix
                .as_ref()
                .map(|f| format!("{:?} on {}", f.action, f.path.segments.join(".")))
                .unwrap_or_else(|| "(none)".to_string());
            report.push_str(&format!(
                "  [{}] Fix: {}  Remaining: C={} H={} M={} L={}\n",
                i + 1,
                fix_str,
                trace.issue_snapshot.critical,
                trace.issue_snapshot.high,
                trace.issue_snapshot.medium,
                trace.issue_snapshot.low,
            ));
        }
    }

    report.push_str(&format!(
        "\nStage: {}\n",
        stage_str(&result.final_version.stage)
    ));

    Ok(Output::text(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;
    use std::fs;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm_converge_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn converge_already_converged_design() {
        let dir = tmp_dir("already_converged");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: build application\n  environment: local\nmetadata:\n  tags: []\n",
        ).unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Status:"));
    }

    #[test]
    fn converge_reports_status_with_empty_use_case() {
        let dir = tmp_dir("empty_use_case");
        fs::write(
            dir.join("design.md"),
            "stage: Context\ncontext:\n  target_user: developer\n  use_case: \"\"\n  environment: local\nmetadata:\n  tags: []\n",
        ).unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Iterations:"));
    }
}
