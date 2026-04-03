use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::refactor::RefactorCandidate;

pub mod exporter;
pub mod function_map;
pub mod gui_dispatch;
pub mod launcher;
pub mod nl_dispatch;
pub mod session;

pub use exporter::{export_structure_view, export_structure_view_from_plan};
pub use function_map::{ViewerAction, ViewerFunction, resolve_action, viewer_function_map};
pub use gui_dispatch::{GuiCommandSpec, dispatch_gui_action};
pub use launcher::{LaunchResult, launch_native_viewer};
pub use nl_dispatch::{NlContext, NlDispatchResult, dispatch_nl};
pub use session::{RefactorSession, attach_session, edit_session, redo_session, undo_session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
}

impl ViewMode {
    pub fn query_value(self) -> &'static str {
        match self {
            Self::TwoD => "2d",
            Self::ThreeD => "3d",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.query_value()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureViewIR {
    pub version: u32,
    pub nodes: Vec<ViewNode>,
    pub edges: Vec<ViewEdge>,
    pub preview: Option<PreviewOverlay>,
    #[serde(default)]
    pub snapshots: Vec<StructureSnapshot>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub risk_overlay: Vec<RiskOverlay>,
    #[serde(default)]
    pub selection: ViewerSelection,
    #[serde(default)]
    pub candidates: Vec<RefactorCandidate>,
    #[serde(default)]
    pub heatmap: Vec<HeatmapDelta>,
    #[serde(default)]
    pub design_sync: DesignSyncStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewNode {
    pub id: String,
    pub label: String,
    pub layer: usize,
    pub role: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewOverlay {
    pub before_graph: PreviewGraph,
    pub after_graph: PreviewGraph,
    pub changed_edges: Vec<ChangedEdge>,
    pub moved_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<PreviewEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedEdge {
    pub from: String,
    pub to: String,
    pub change: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureSnapshot {
    pub before: SnapshotGraph,
    pub after: SnapshotGraph,
    pub delta: SnapshotDelta,
    pub timestamp: String,
    pub action: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotGraph {
    pub nodes: Vec<ViewNode>,
    pub edges: Vec<ViewEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub summary: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub snapshot_index: usize,
    pub action: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskOverlay {
    pub target: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerSelection {
    #[serde(default)]
    pub selected_nodes: Vec<String>,
    #[serde(default)]
    pub selected_edges: Vec<crate::refactor::StructureEdge>,
    pub selection_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapDelta {
    pub target: String,
    pub color: String,
    pub label: String,
    pub magnitude: f32,
}

impl Default for HeatmapDelta {
    fn default() -> Self {
        Self {
            target: String::new(),
            color: "blue".to_string(),
            label: String::new(),
            magnitude: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DesignSyncStatus {
    pub design_md_updated: bool,
    pub report_md_updated: bool,
    pub ir_updated: bool,
    #[serde(default)]
    pub last_delta: Vec<String>,
}

impl Default for ViewerSelection {
    fn default() -> Self {
        Self {
            selected_nodes: Vec::new(),
            selected_edges: Vec::new(),
            selection_mode: "single".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ViewerLoopTelemetry {
    pub watcher_count: usize,
    pub websocket_count: usize,
    pub polling_loop_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StructureViewReport {
    pub root: String,
    pub mode: ViewMode,
    pub ir_path: String,
    pub launch_url: String,
    pub launched: bool,
    pub viewer_loop: ViewerLoopTelemetry,
    pub ir: StructureViewIR,
}

pub fn structure_ir_path(root: &Path) -> PathBuf {
    root.join(".dbm").join("structure_view.json")
}

pub fn session_path(root: &Path) -> PathBuf {
    root.join(".dbm").join("structure_session.json")
}

pub fn gui_action_path(root: &Path) -> PathBuf {
    root.join(".dbm").join("gui_action.json")
}
