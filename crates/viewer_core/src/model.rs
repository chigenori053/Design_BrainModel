use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoD => "2d",
            Self::ThreeD => "3d",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ViewerLoopTelemetry {
    pub watcher_count: usize,
    pub websocket_count: usize,
    pub polling_loop_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedModuleId {
    pub crate_name: String,
    pub module_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleNode {
    pub qualified_id: QualifiedModuleId,
    pub logical_name: String,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefactorActionKind {
    ExtractInterface,
    RemoveDependency,
    SplitModule,
    MergeModule,
    MoveFile,
    RenameBoundary,
    IntroduceService,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuiActionMode {
    Preview,
    Apply,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefactorCandidate {
    pub kind: RefactorActionKind,
    pub title: String,
    pub rationale: String,
    pub confidence_milli: u16,
    pub from_node: ModuleNode,
    pub to_node: ModuleNode,
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
pub struct StructureEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerSelection {
    #[serde(default)]
    pub selected_nodes: Vec<String>,
    #[serde(default)]
    pub selected_edges: Vec<StructureEdge>,
    pub selection_mode: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ValidationOverlay {
    pub cycle_count: usize,
    pub layer_violations: usize,
    #[serde(default)]
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ActionRequest {
    pub target: String,
    pub node: Option<String>,
    pub selected_nodes: Vec<String>,
    pub mode: GuiActionMode,
}

pub type DispatchAction =
    Arc<dyn Fn(ActionRequest) -> Result<String, String> + Send + Sync + 'static>;
pub type SourcePathResolver =
    Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync + 'static>;

/// NLプロンプトをCLIへ送り結果を受け取るコールバック
/// (prompt, selected_node) -> response_json (NlDispatchResult相当)
pub type DispatchNl =
    Arc<dyn Fn(&str, Option<&str>) -> Result<String, String> + Send + Sync + 'static>;
