use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{
    list_history_snapshots, load_baseline, load_design_doc, make_initial_version, resolve_root,
};
use unified_design_ir::{ChangeType, diff_versions};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("diff", execute)
}

/// /design diff [path]
///
/// 前バージョンとの差分を表示する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));

    // Try to get the two most recent snapshots from history
    let snapshots = list_history_snapshots(&root);
    if snapshots.len() >= 2 {
        let before = &snapshots[snapshots.len() - 2];
        let after = &snapshots[snapshots.len() - 1];
        return render_diff(before, after);
    }

    // Fall back: baseline vs. current design.md
    let doc = load_design_doc(&root).map_err(|e| CommandError::ExecutionError(e))?;
    let stage = doc.stage.clone();

    let baseline = load_baseline(&root)
        .filter(|v| v.stage == stage)
        .unwrap_or_else(|| make_initial_version(super::default_baseline_for_stage(&stage)).0);

    let (current, _) = make_initial_version(doc);

    render_diff(&baseline, &current)
}

fn render_diff(
    before: &unified_design_ir::DesignVersion,
    after: &unified_design_ir::DesignVersion,
) -> Result<Output, CommandError> {
    if before.stage != after.stage {
        return Ok(Output::text(
            "Cannot diff: stage changed between versions.".to_string(),
        ));
    }

    let diff_result = diff_versions(before, after)
        .map_err(|e| CommandError::ExecutionError(format!("Diff error: {e:?}")))?;

    if diff_result.changes.is_empty() {
        return Ok(Output::text("No changes between versions.".to_string()));
    }

    let mut report = format!("v{} → v{}\n", before.id.seq, after.id.seq);
    report.push_str(&format!(
        "Changes: +{} -{} ~{}\n\n",
        diff_result.summary.added, diff_result.summary.removed, diff_result.summary.modified
    ));

    for change in &diff_result.changes {
        let path = change.path.segments.join(".");
        let before_str = change
            .before
            .as_ref()
            .map(|v| format!("{v}"))
            .unwrap_or_default();
        let after_str = change
            .after
            .as_ref()
            .map(|v| format!("{v}"))
            .unwrap_or_default();

        let line = match change.change_type {
            ChangeType::Added => format!("  + {path}: {after_str}\n"),
            ChangeType::Removed => format!("  - {path}: {before_str}\n"),
            ChangeType::Modified => format!("  ~ {path}: {before_str} → {after_str}\n"),
            ChangeType::Moved => format!("  ↻ {path}\n"),
        };
        report.push_str(&line);
    }

    Ok(Output::text(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unified_design_ir::{ContextSpec, DesignDocument, Metadata, Stage};

    fn context_doc(target_user: &str, use_case: &str) -> DesignDocument {
        DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some(target_user.to_string()),
                use_case: Some(use_case.to_string()),
                environment: None,
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        }
    }

    #[test]
    fn render_diff_same_docs_is_empty() {
        let (v1, _) = make_initial_version(context_doc("developer", "build app"));
        let (v2, _) = make_initial_version(context_doc("developer", "build app"));
        let out = render_diff(&v1, &v2).unwrap();
        assert!(out.message.contains("No changes"));
    }

    #[test]
    fn render_diff_detects_modification() {
        let (v1, _) = make_initial_version(context_doc("developer", "build app"));
        let (v2, _) = make_initial_version(context_doc("scientist", "build app"));
        let out = render_diff(&v1, &v2).unwrap();
        assert!(out.message.contains("target_user") || out.message.contains("Changes"));
    }
}
