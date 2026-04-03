use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use viewer_core::model::{
    ActionRequest, GuiActionMode as CoreGuiActionMode, SourcePathResolver, ValidationIssue,
    ValidationOverlay, ViewMode as CoreViewMode,
};
use viewer_core::native::{LaunchRequest, launch_native_viewer as launch_embedded_viewer};

use crate::viewer::nl_dispatch::{NlContext, dispatch_nl};

use super::{ViewMode, ViewerLoopTelemetry};
use crate::refactor::{GuiAction, GuiActionMode};
use crate::service::{analyze_path, build_validation_report};
use crate::source_index::ModuleSourceIndex;
use crate::viewer::dispatch_gui_action;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchResult {
    pub url: String,
    pub launched: bool,
    pub telemetry: ViewerLoopTelemetry,
}

pub fn launch_native_viewer(
    mode: ViewMode,
    root: &Path,
    ir_path: &Path,
) -> Result<LaunchResult, String> {
    let analysis = analyze_path(root)?;
    let source_index = ModuleSourceIndex::build(root).unwrap_or_default();
    let graph_nodes = analysis.graph_nodes.clone();
    let diagnostics = validation_overlay(root)?;
    let dispatch_root = root.to_path_buf();
    let resolve_root = root.to_path_buf();

    let dispatch_action = Arc::new(move |request: ActionRequest| {
        let event = GuiAction {
            action: "refactor".to_string(),
            target: request.target,
            node: request.node,
            project_root: Some(dispatch_root.clone()),
            selected_nodes: request.selected_nodes,
            selected_edges: Vec::new(),
            mode: match request.mode {
                CoreGuiActionMode::Preview => GuiActionMode::Preview,
                CoreGuiActionMode::Apply => GuiActionMode::Apply,
            },
        };
        let (command, refreshed) = dispatch_gui_action(&dispatch_root, event)?;
        Ok(format!(
            "{} {} [{} nodes / {} edges]",
            command.command_kind,
            command.stage,
            refreshed.nodes.len(),
            refreshed.edges.len()
        ))
    });

    let source_path_for_node: SourcePathResolver = Arc::new(move |logical_name: &str| {
        graph_nodes
            .iter()
            .find(|node| node.logical_name == logical_name)
            .and_then(|node| node.source_path.clone())
            .or_else(|| {
                source_index
                    .bind_graph_node(logical_name)
                    .map(|(_, path)| resolve_root.join(path))
            })
    });

    let nl_root = root.to_path_buf();
    let dispatch_nl_cb = std::sync::Arc::new(move |prompt: &str, selected_node: Option<&str>| {
        let ctx = NlContext {
            prompt: prompt.to_string(),
            selected_node: selected_node.map(str::to_string),
            root: nl_root.clone(),
        };
        let result = dispatch_nl(&ctx);
        serde_json::to_string(&result).map_err(|e| e.to_string())
    });

    let launch = launch_embedded_viewer(LaunchRequest {
        mode: to_core_mode(mode),
        ir_path: ir_path.to_path_buf(),
        root: root.to_path_buf(),
        diagnostics,
        dispatch_action,
        source_path_for_node,
        dispatch_nl: dispatch_nl_cb,
    })?;

    Ok(LaunchResult {
        url: launch.url,
        launched: launch.launched,
        telemetry: ViewerLoopTelemetry {
            watcher_count: launch.telemetry.watcher_count,
            websocket_count: launch.telemetry.websocket_count,
            polling_loop_count: launch.telemetry.polling_loop_count,
        },
    })
}

fn validation_overlay(root: &Path) -> Result<ValidationOverlay, String> {
    let report = build_validation_report(root)?;
    Ok(ValidationOverlay {
        cycle_count: report.cycles.cycles.len(),
        layer_violations: report.violations.len(),
        issues: report
            .issues
            .into_iter()
            .map(|message| ValidationIssue {
                severity: "error".to_string(),
                message,
            })
            .chain(report.warnings.into_iter().map(|message| ValidationIssue {
                severity: "warning".to_string(),
                message,
            }))
            .collect(),
    })
}

fn to_core_mode(mode: ViewMode) -> CoreViewMode {
    match mode {
        ViewMode::TwoD => CoreViewMode::TwoD,
        ViewMode::ThreeD => CoreViewMode::ThreeD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_builds_embedded_descriptor() {
        unsafe {
            std::env::set_var("DBM_VIEWER_SKIP_OPEN", "1");
        }
        let result = launch_native_viewer(
            ViewMode::ThreeD,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            Path::new("/tmp/sample/.dbm/structure_view.json"),
        )
        .unwrap_or(LaunchResult {
            url: "embedded://viewer_core?mode=3d".to_string(),
            launched: false,
            telemetry: ViewerLoopTelemetry::default(),
        });
        assert!(result.url.contains("embedded://viewer_core"));
        assert!(result.url.contains("mode=3d"));
        assert!(result.url.contains("structure_view.json"));
        assert!(!result.launched);
        unsafe {
            std::env::remove_var("DBM_VIEWER_SKIP_OPEN");
        }
    }
}
