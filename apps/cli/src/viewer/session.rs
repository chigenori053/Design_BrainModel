use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use viewer_core::timeline::compact_delta_chain;

use crate::refactor::{
    GuiAction, GuiActionMode, RefactorRuntimeOptions, build_apply_report,
    gui_event_to_plan_with_candidates,
};

use super::{
    DesignSyncStatus, EdgeDeltaDelta, HeatmapDelta, HistoryEntry, NodeDelta, OverlayDelta,
    RiskOverlay, SnapshotDelta, SnapshotGraph, StructureSnapshot, StructureViewIR, ViewMode,
    ViewerLoopTelemetry, ViewerSelection, export_structure_view, export_structure_view_from_plan,
    launch_native_viewer, session_path, structure_ir_path, sync_apply_preview_with_selection,
    sync_preview_with_selection, sync_transaction_execution_with_selection,
    sync_transaction_preview_with_selection,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefactorSession {
    pub session_id: String,
    pub project_path: String,
    pub current_ir: StructureViewIR,
    pub history_stack: Vec<SessionRecord>,
    pub redo_stack: Vec<SessionRecord>,
    pub snapshot_index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub snapshot: StructureSnapshot,
    pub files: Vec<FileVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileVersion {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionCommandReport {
    pub session_id: String,
    pub root: String,
    pub ir_path: String,
    pub snapshot_index: usize,
    pub history_depth: usize,
    pub redo_depth: usize,
    pub launch_url: Option<String>,
    pub viewer_loop: ViewerLoopTelemetry,
}

pub fn attach_session(root: &Path) -> Result<RefactorSession, String> {
    if session_path(root).exists() {
        load_session(root)
    } else {
        let ir = export_structure_view(root)?;
        let session = RefactorSession {
            session_id: new_session_id(),
            project_path: root.display().to_string(),
            current_ir: ir,
            history_stack: Vec::new(),
            redo_stack: Vec::new(),
            snapshot_index: 0,
        };
        save_session(root, &session)?;
        Ok(session)
    }
}

pub fn edit_session(root: &Path, mode: ViewMode) -> Result<SessionCommandReport, String> {
    let session = attach_session(root)?;
    write_ir(root, &session.current_ir)?;
    let ir_path = structure_ir_path(root);
    let launch = launch_native_viewer(mode, root, &ir_path)?;
    Ok(SessionCommandReport {
        session_id: session.session_id,
        root: root.display().to_string(),
        ir_path: ir_path.display().to_string(),
        snapshot_index: session.snapshot_index,
        history_depth: session.history_stack.len(),
        redo_depth: session.redo_stack.len(),
        launch_url: Some(launch.url),
        viewer_loop: launch.telemetry,
    })
}

pub fn apply_session_action(
    root: &Path,
    event: GuiAction,
) -> Result<(RefactorSession, StructureViewIR), String> {
    let mut session = attach_session(root)?;
    let (plan, candidates) = gui_event_to_plan_with_candidates(event.clone())?;
    let before_ir = session.current_ir.clone();
    if matches!(event.mode, GuiActionMode::Preview) {
        let mut preview_ir = export_structure_view_from_plan(root, &plan)?;
        preview_ir.snapshots = compacted_history_snapshots(&session.history_stack);
        preview_ir.history = history_entries(&session.history_stack);
        decorate_ir(
            &mut preview_ir,
            &event,
            candidates,
            heatmap(&plan),
            DesignSyncStatus::default(),
        );
        session.current_ir = preview_ir.clone();
        save_session(root, &session)?;
        write_ir(root, &preview_ir)?;
        return Ok((session, preview_ir));
    }

    let files = capture_before_files(root, &plan.affected_files)?;
    let report = build_apply_report(
        &plan,
        &RefactorRuntimeOptions {
            auto_commit: false,
            no_build: false,
            backup: true,
            format: false,
        },
    )?;
    if !report.apply.applied {
        return Err("refactor apply was blocked by safety gate".to_string());
    }
    let delta = architecture_delta_lines(&plan);
    let mut after_ir = export_structure_view_from_plan(root, &plan)?;
    let record = SessionRecord {
        snapshot: StructureSnapshot {
            base: checkpoint_base(&before_ir, session.history_stack.len()),
            delta: build_snapshot_delta(&before_ir, &after_ir, &delta),
            timestamp: timestamp_now(),
            action: format!("{:?}", plan.target),
            confidence: plan.confidence,
        },
        files: capture_after_files(root, files)?,
    };
    session.history_stack.push(record.clone());
    session.redo_stack.clear();
    session.snapshot_index = session.history_stack.len();
    after_ir.snapshots = compacted_history_snapshots(&session.history_stack);
    after_ir.history = history_entries(&session.history_stack);
    after_ir.risk_overlay = risk_overlay(&report.validation, &report.plan, &report.apply);
    decorate_ir(
        &mut after_ir,
        &event,
        candidates,
        heatmap(&plan),
        sync_design_docs(root, &delta)?,
    );
    session.current_ir = after_ir.clone();
    save_session(root, &session)?;
    write_ir(root, &after_ir)?;
    Ok((session, after_ir))
}

pub fn undo_session(root: &Path) -> Result<SessionCommandReport, String> {
    let mut session = attach_session(root)?;
    let Some(record) = session.history_stack.pop() else {
        return Err("no undo snapshot available".to_string());
    };
    restore_file_versions(root, &record.files, true)?;
    session.redo_stack.push(record);
    session.snapshot_index = session.history_stack.len();
    session.current_ir = export_structure_view(root)?;
    session.current_ir.snapshots = compacted_history_snapshots(&session.history_stack);
    session.current_ir.history = history_entries(&session.history_stack);
    save_session(root, &session)?;
    write_ir(root, &session.current_ir)?;
    Ok(report_from_session(root, &session, None))
}

pub fn redo_session(root: &Path) -> Result<SessionCommandReport, String> {
    let mut session = attach_session(root)?;
    let Some(record) = session.redo_stack.pop() else {
        return Err("no redo snapshot available".to_string());
    };
    restore_file_versions(root, &record.files, false)?;
    session.history_stack.push(record.clone());
    session.snapshot_index = session.history_stack.len();
    session.current_ir = export_structure_view(root)?;
    session.current_ir.snapshots = compacted_history_snapshots(&session.history_stack);
    session.current_ir.history = history_entries(&session.history_stack);
    session.current_ir.risk_overlay = vec![RiskOverlay {
        target: record.snapshot.action.clone(),
        level: "info".to_string(),
        message: "redo reapplied snapshot".to_string(),
    }];
    save_session(root, &session)?;
    write_ir(root, &session.current_ir)?;
    Ok(report_from_session(root, &session, None))
}

fn report_from_session(
    root: &Path,
    session: &RefactorSession,
    launch_url: Option<String>,
) -> SessionCommandReport {
    SessionCommandReport {
        session_id: session.session_id.clone(),
        root: root.display().to_string(),
        ir_path: structure_ir_path(root).display().to_string(),
        snapshot_index: session.snapshot_index,
        history_depth: session.history_stack.len(),
        redo_depth: session.redo_stack.len(),
        launch_url,
        viewer_loop: ViewerLoopTelemetry::default(),
    }
}

fn load_session(root: &Path) -> Result<RefactorSession, String> {
    let raw = fs::read_to_string(session_path(root))
        .map_err(|err| format!("failed to read session: {err}"))?;
    serde_json::from_str(&raw).map_err(|err| format!("invalid session JSON: {err}"))
}

fn save_session(root: &Path, session: &RefactorSession) -> Result<(), String> {
    let path = session_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(session).map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("failed to write session: {err}"))
}

fn write_ir(root: &Path, ir: &StructureViewIR) -> Result<(), String> {
    let path = structure_ir_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(ir).map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("failed to write IR: {err}"))
}

fn snapshot_graph(ir: &StructureViewIR) -> SnapshotGraph {
    SnapshotGraph {
        nodes: ir.nodes.clone(),
        edges: ir.edges.clone(),
    }
}

fn checkpoint_base(ir: &StructureViewIR, history_len: usize) -> Option<SnapshotGraph> {
    if history_len.is_multiple_of(20) {
        Some(snapshot_graph(ir))
    } else {
        None
    }
}

fn build_snapshot_delta(
    before_ir: &StructureViewIR,
    after_ir: &StructureViewIR,
    summary: &[String],
) -> SnapshotDelta {
    let before_nodes = before_ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_nodes = after_ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let node_ids = before_nodes
        .keys()
        .chain(after_nodes.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    let node_updates = node_ids
        .into_iter()
        .filter_map(|id| {
            let before = before_nodes.get(&id).cloned();
            let after = after_nodes.get(&id).cloned();
            (before != after).then_some(NodeDelta { id, before, after })
        })
        .collect();

    let before_edges = before_ir
        .edges
        .iter()
        .map(|edge| (edge_key(edge), edge.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_edges = after_ir
        .edges
        .iter()
        .map(|edge| (edge_key(edge), edge.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let edge_keys = before_edges
        .keys()
        .chain(after_edges.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    let edge_updates = edge_keys
        .into_iter()
        .filter_map(|key| {
            let before = before_edges.get(&key).cloned();
            let after = after_edges.get(&key).cloned();
            if before == after {
                return None;
            }
            let (from, to, kind) = split_edge_key(&key);
            Some(EdgeDeltaDelta {
                from,
                to,
                kind,
                before,
                after,
            })
        })
        .collect();

    let before_overlay = before_ir
        .risk_overlay
        .iter()
        .map(|overlay| (overlay.target.clone(), overlay.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_overlay = after_ir
        .risk_overlay
        .iter()
        .map(|overlay| (overlay.target.clone(), overlay.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let overlay_targets = before_overlay
        .keys()
        .chain(after_overlay.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    let overlay_updates = overlay_targets
        .into_iter()
        .filter_map(|target| {
            let before = before_overlay.get(&target).cloned();
            let after = after_overlay.get(&target).cloned();
            (before != after).then_some(OverlayDelta {
                target,
                before,
                after,
            })
        })
        .collect();

    SnapshotDelta {
        summary: summary.to_vec(),
        node_updates,
        edge_updates,
        overlay_updates,
    }
}

fn edge_key(edge: &crate::viewer::ViewEdge) -> String {
    format!("{}|{}|{}", edge.from, edge.to, edge.kind)
}

fn split_edge_key(key: &str) -> (String, String, String) {
    let mut parts = key.split('|');
    (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )
}

fn history_entries(history: &[SessionRecord]) -> Vec<HistoryEntry> {
    history
        .iter()
        .enumerate()
        .map(|(index, entry)| HistoryEntry {
            snapshot_index: index + 1,
            action: entry.snapshot.action.clone(),
            confidence: format!("{:.2}", entry.snapshot.confidence),
        })
        .collect()
}

fn compacted_history_snapshots(history: &[SessionRecord]) -> Vec<StructureSnapshot> {
    let core_snapshots = history
        .iter()
        .map(|entry| entry.snapshot.clone())
        .map(StructureSnapshot::into_core)
        .collect::<Vec<_>>();
    compact_delta_chain(&core_snapshots, 100)
        .into_iter()
        .map(StructureSnapshot::from_core)
        .collect()
}

fn capture_before_files(root: &Path, files: &[PathBuf]) -> Result<Vec<FileVersion>, String> {
    let mut out = Vec::new();
    for file in files {
        let absolute = if file.is_absolute() {
            file.clone()
        } else {
            root.join(file)
        };
        let before = if absolute.exists() {
            Some(
                fs::read_to_string(&absolute)
                    .map_err(|err| format!("failed to read {}: {err}", absolute.display()))?,
            )
        } else {
            None
        };
        let relative = absolute
            .strip_prefix(root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| absolute.display().to_string());
        out.push(FileVersion {
            path: relative,
            before,
            after: None,
        });
    }
    Ok(out)
}

fn capture_after_files(
    root: &Path,
    mut files: Vec<FileVersion>,
) -> Result<Vec<FileVersion>, String> {
    for file in &mut files {
        let absolute = root.join(&file.path);
        file.after = if absolute.exists() {
            Some(
                fs::read_to_string(&absolute)
                    .map_err(|err| format!("failed to read {}: {err}", absolute.display()))?,
            )
        } else {
            None
        };
    }
    Ok(files)
}

fn restore_file_versions(
    root: &Path,
    files: &[FileVersion],
    restore_before: bool,
) -> Result<(), String> {
    for file in files {
        let absolute = root.join(&file.path);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        match if restore_before {
            &file.before
        } else {
            &file.after
        } {
            Some(content) => fs::write(&absolute, content)
                .map_err(|err| format!("failed to write {}: {err}", absolute.display()))?,
            None => {
                if absolute.exists() {
                    fs::remove_file(&absolute)
                        .map_err(|err| format!("failed to remove {}: {err}", absolute.display()))?;
                }
            }
        }
    }
    Ok(())
}

fn architecture_delta_lines(plan: &crate::refactor::RefactorPlan) -> Vec<String> {
    let mut lines = Vec::new();
    for edge in &plan.removed_edges {
        lines.push(format!("Removed cycle: {} ↔ {}", edge.from, edge.to));
        lines.push(format!(
            "Added interface: {}{}Interface",
            capitalize(&edge.from),
            capitalize(&edge.to)
        ));
    }
    for (from, to) in &plan.moved_files {
        lines.push(format!(
            "Moved file: {} -> {}",
            from.display(),
            to.display()
        ));
    }
    if lines.is_empty() {
        lines.push(format!("Applied action: {:?}", plan.target));
    }
    lines
}

fn risk_overlay(
    validation: &crate::refactor::ValidationResult,
    plan: &crate::refactor::RefactorPlan,
    apply: &crate::refactor::ApplyResult,
) -> Vec<RiskOverlay> {
    let mut overlays = vec![
        RiskOverlay {
            target: format!("{:?}", plan.target),
            level: if validation.cycle_removed {
                "green"
            } else {
                "red"
            }
            .to_string(),
            message: format!("cycle delta: {}", validation.cycle_removed),
        },
        RiskOverlay {
            target: format!("{:?}", plan.target),
            level: if apply.build_ok { "green" } else { "red" }.to_string(),
            message: format!("build validation: {}", apply.build_ok),
        },
        RiskOverlay {
            target: format!("{:?}", plan.target),
            level: if validation.no_new_layer_violation {
                "green"
            } else {
                "red"
            }
            .to_string(),
            message: format!(
                "semantic variance / layering stable: {}",
                validation.no_new_layer_violation
            ),
        },
    ];
    if !apply.applied {
        overlays.push(RiskOverlay {
            target: format!("{:?}", plan.target),
            level: "red".to_string(),
            message: "rollback point consumed without successful apply".to_string(),
        });
    }
    overlays
}

fn decorate_ir(
    ir: &mut StructureViewIR,
    event: &GuiAction,
    candidates: Vec<crate::refactor::RefactorCandidate>,
    heatmap: Vec<HeatmapDelta>,
    design_sync: DesignSyncStatus,
) {
    ir.selection = ViewerSelection {
        selected_nodes: event.selected_nodes.clone(),
        selected_edges: event.selected_edges.clone(),
        selection_mode: if event.selected_nodes.len() + event.selected_edges.len() > 1 {
            "multi".to_string()
        } else {
            "single".to_string()
        },
    };
    ir.candidates = candidates;
    ir.heatmap = heatmap;
    ir.design_sync = design_sync;
    sync_preview_with_selection(ir);
    sync_apply_preview_with_selection(ir);
    sync_transaction_preview_with_selection(ir);
    sync_transaction_execution_with_selection(ir);
}

fn heatmap(plan: &crate::refactor::RefactorPlan) -> Vec<HeatmapDelta> {
    let mut heatmap = Vec::new();
    if !plan.removed_edges.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "green".to_string(),
            label: "reduced coupling".to_string(),
            magnitude: 0.92,
        });
    }
    if !plan.moved_files.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "blue".to_string(),
            label: "moved responsibility".to_string(),
            magnitude: 0.74,
        });
    }
    if heatmap.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "red".to_string(),
            label: "new risk".to_string(),
            magnitude: 0.48,
        });
    }
    heatmap
}

fn sync_design_docs(root: &Path, delta: &[String]) -> Result<DesignSyncStatus, String> {
    let block = format!(
        "## Architecture Delta\n{}\n",
        delta
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut updated = Vec::new();
    for relative in ["design.md", "report.md"] {
        let path = root.join(relative);
        fs::write(&path, &block)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        updated.push(relative.to_string());
    }
    Ok(DesignSyncStatus {
        design_md_updated: updated.iter().any(|entry| entry == "design.md"),
        report_md_updated: updated.iter().any(|entry| entry == "report.md"),
        ir_updated: true,
        last_delta: delta.to_vec(),
    })
}

fn new_session_id() -> String {
    format!("session-{}", timestamp_now())
}

fn timestamp_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(name: &str) -> PathBuf {
        let unique = format!(
            "{name}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"viewer_session\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("debug");
        root
    }

    #[test]
    fn attach_creates_session_and_ir() {
        let root = sample_project("attach");
        let session = attach_session(&root).expect("session");
        assert_eq!(session.current_ir.version, 2);
        assert!(session_path(&root).exists());
    }

    #[test]
    fn undo_redo_restores_session_consistently() {
        let root = sample_project("undo_redo");
        let _ = apply_session_action(
            &root,
            GuiAction {
                action: "refactor".to_string(),
                target: "cycle".to_string(),
                node: Some("renderer".to_string()),
                project_root: Some(root.clone()),
                selected_nodes: Vec::new(),
                selected_edges: Vec::new(),
                mode: crate::refactor::GuiActionMode::Apply,
            },
        )
        .expect("apply");
        let undo = undo_session(&root).expect("undo");
        assert_eq!(undo.history_depth, 0);
        assert_eq!(undo.redo_depth, 1);
        let redo = redo_session(&root).expect("redo");
        assert_eq!(redo.history_depth, 1);
    }
}
