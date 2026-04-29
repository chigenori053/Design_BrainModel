use std::fs;
use std::path::Path;

use serde::Serialize;
use viewer_core::replay_export::{ReplayExportManifest, export_demo_replay};
use viewer_core::timeline::{compact_delta_chain, rebuild_scene_from_deltas};

use super::{attach_session, export_structure_view, structure_ir_path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayCommandReport {
    pub root: String,
    pub tick: usize,
    pub reverse: bool,
    pub available_ticks: usize,
    pub nodes: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimelineCommandReport {
    pub root: String,
    pub checkpoints: usize,
    pub ticks: usize,
    pub latest_tick: usize,
    pub compacted_ticks: usize,
}

pub fn summarize_timeline(root: &Path) -> Result<TimelineCommandReport, String> {
    let session = attach_session(root)?;
    let core_snapshots = session
        .current_ir
        .snapshots
        .iter()
        .cloned()
        .map(super::StructureSnapshot::into_core)
        .collect::<Vec<_>>();
    let compacted = compact_delta_chain(&core_snapshots, 100);
    Ok(TimelineCommandReport {
        root: root.display().to_string(),
        checkpoints: compacted
            .iter()
            .filter(|snapshot| snapshot.base.is_some())
            .count(),
        ticks: session.current_ir.snapshots.len(),
        latest_tick: session.current_ir.snapshots.len().saturating_sub(1),
        compacted_ticks: compacted.len(),
    })
}

pub fn replay_structure(
    root: &Path,
    tick: Option<usize>,
    reverse: bool,
) -> Result<ReplayCommandReport, String> {
    let session = attach_session(root)?;
    let core_snapshots = session
        .current_ir
        .snapshots
        .iter()
        .cloned()
        .map(super::StructureSnapshot::into_core)
        .collect::<Vec<_>>();
    let compacted = compact_delta_chain(&core_snapshots, 100);
    let rebuilt = rebuild_scene_from_deltas(&compacted);
    let resolved_tick = tick
        .unwrap_or_else(|| rebuilt.len().saturating_sub(1))
        .min(rebuilt.len().saturating_sub(1));
    let index = if reverse {
        rebuilt
            .len()
            .saturating_sub(1)
            .saturating_sub(resolved_tick)
    } else {
        resolved_tick
    };
    let graph = rebuilt.get(index).cloned().unwrap_or_else(|| {
        let core = session.current_ir.to_core();
        viewer_core::model::SnapshotGraph {
            nodes: core.nodes,
            edges: core.edges,
        }
    });
    Ok(ReplayCommandReport {
        root: root.display().to_string(),
        tick: resolved_tick,
        reverse,
        available_ticks: rebuilt.len(),
        nodes: graph.nodes.len(),
        edges: graph.edges.len(),
    })
}

pub fn export_demo_replay_assets(
    root: &Path,
    export_dir: Option<&Path>,
) -> Result<ReplayExportManifest, String> {
    let session = attach_session(root)?;
    let mut ir = session.current_ir.clone();
    if ir.scene_3d.is_none() {
        ir = export_structure_view(root)?;
    }
    let core_ir = ir.to_core();
    let compacted = compact_delta_chain(&core_ir.snapshots.to_vec(), 100);
    let dir = export_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(".dbm").join("replay"));
    let manifest = export_demo_replay(&core_ir, &compacted, &dir)?;

    let ir_path = structure_ir_path(root);
    if let Some(parent) = ir_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    Ok(manifest)
}
