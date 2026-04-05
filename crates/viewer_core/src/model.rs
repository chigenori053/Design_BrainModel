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
    pub preview: Option<PreviewDiff>,
    #[serde(default)]
    pub apply_preview: Option<ApplyPreviewPlan>,
    #[serde(default)]
    pub transaction_preview: Option<TransactionPreview>,
    #[serde(default)]
    pub transaction_execution: Option<TransactionExecutionPreview>,
    #[serde(default)]
    pub transaction_result: Option<TransactionResult>,
    #[serde(default)]
    pub promote_result: Option<PromoteResult>,
    #[serde(default)]
    pub git_commit_preview: Option<GitCommitPreview>,
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
    #[serde(default)]
    pub scene_3d: Option<Structure3DIr>,
}

impl Default for StructureViewIR {
    fn default() -> Self {
        Self {
            version: 2,
            nodes: Vec::new(),
            edges: Vec::new(),
            preview: None,
            apply_preview: None,
            transaction_preview: None,
            transaction_execution: None,
            transaction_result: None,
            promote_result: None,
            git_commit_preview: None,
            snapshots: Vec::new(),
            history: Vec::new(),
            risk_overlay: Vec::new(),
            selection: ViewerSelection::default(),
            candidates: Vec::new(),
            heatmap: Vec::new(),
            design_sync: DesignSyncStatus::default(),
            scene_3d: None,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewDiff {
    pub candidate_id: String,
    pub summary: String,
    pub estimated_effect: String,
    pub safe: bool,
    pub diff_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyPreviewPlan {
    pub candidate_id: String,
    pub target_files: Vec<String>,
    pub operations: Vec<String>,
    pub checks: Vec<String>,
    pub rollback: RollbackPreview,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPreview {
    pub mode: String,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionPreview {
    pub candidate_id: String,
    pub allowed: bool,
    pub safe: bool,
    pub steps: Vec<String>,
    pub rollback_strategy: TransactionRollbackPreview,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRollbackPreview {
    pub mode: String,
    pub guaranteed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionExecutionPreview {
    pub candidate_id: String,
    pub allowed: bool,
    pub executed: bool,
    pub sandbox_write: SandboxWritePreview,
    pub steps: Vec<String>,
    pub rollback_guaranteed: bool,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxWritePreview {
    pub enabled: bool,
    pub target_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionResult {
    pub executed: bool,
    pub success: bool,
    pub sandbox_root: String,
    pub written_files: Vec<String>,
    pub cargo_check: String,
    pub rollback_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromoteResult {
    pub confirmed: bool,
    pub workspace_write: bool,
    pub written_files: Vec<String>,
    pub cargo_check: String,
    pub rollback_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitPreview {
    pub branch: String,
    pub protected_branch: bool,
    pub commit_allowed: bool,
    pub commit_message: String,
    pub changed_files: Vec<String>,
    pub push: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureSnapshot {
    #[serde(default)]
    pub base: Option<SnapshotGraph>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SnapshotDelta {
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub node_updates: Vec<NodeDelta>,
    #[serde(default)]
    pub edge_updates: Vec<EdgeDeltaDelta>,
    #[serde(default)]
    pub overlay_updates: Vec<OverlayDelta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDelta {
    pub id: String,
    pub before: Option<ViewNode>,
    pub after: Option<ViewNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeDeltaDelta {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub before: Option<ViewEdge>,
    pub after: Option<ViewEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayDelta {
    pub target: String,
    pub before: Option<RiskOverlay>,
    pub after: Option<RiskOverlay>,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Structure3DIr {
    pub graph: SemanticGraph3D,
    #[serde(default)]
    pub runtime_paths: Vec<RuntimePath3D>,
    #[serde(default)]
    pub overlays: ViewerOverlays3D,
    #[serde(default)]
    pub timeline: Timeline3D,
    #[serde(default)]
    pub camera: CameraPreset3D,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticGraph3D {
    #[serde(default)]
    pub nodes: Vec<Node3D>,
    #[serde(default)]
    pub edges: Vec<Edge3D>,
    #[serde(default)]
    pub clusters: Vec<Cluster3D>,
    #[serde(default)]
    pub layers: Vec<LayerPlane3D>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Node3D {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub position: Vec3,
    pub size: f32,
    pub importance: f32,
    pub heat: f32,
    pub source_binding: Option<SourceBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Edge3D {
    pub from: String,
    pub to: String,
    pub weight: f32,
    pub edge_kind: String,
    pub violation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Cluster3D {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LayerPlane3D {
    pub level: usize,
    pub label: String,
    pub axis_x: f32,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuntimePath3D {
    pub id: String,
    #[serde(default)]
    pub points: Vec<Vec3>,
    pub path_kind: RuntimePathKind,
    pub animated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RuntimePathKind {
    #[default]
    Execution,
    Validation,
    Rollback,
    MemoryRelease,
    RefactorPreview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ViewerOverlays3D {
    pub refactor: Option<RefactorOverlay3D>,
    pub telemetry: Option<TelemetryOverlay3D>,
    pub source_jump: bool,
    pub design_sync: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RefactorOverlay3D {
    #[serde(default)]
    pub selected_nodes: Vec<String>,
    #[serde(default)]
    pub candidate_moves: Vec<CandidateMove3D>,
    pub predicted_cycle_reduction: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CandidateMove3D {
    pub node_id: String,
    pub from: Vec3,
    pub to: Vec3,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TelemetryOverlay3D {
    pub hot_path_count: usize,
    pub rollback_count: usize,
    pub memory_release_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Timeline3D {
    #[serde(default)]
    pub snapshots: Vec<GraphSnapshot3D>,
    pub current_tick: usize,
    pub autoplay: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GraphSnapshot3D {
    pub label: String,
    pub tick: usize,
    #[serde(default)]
    pub animation: GraphDeltaAnimation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CameraPreset3D {
    pub focus_cluster: Option<String>,
    pub mode: CameraMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CameraMode {
    #[default]
    Architectural,
    RuntimeFlow,
    RefactorPreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBinding {
    pub file: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GraphDeltaAnimation {
    #[serde(default)]
    pub moved_nodes: Vec<NodeMoveDelta>,
    #[serde(default)]
    pub added_edges: Vec<EdgeDelta>,
    #[serde(default)]
    pub removed_edges: Vec<EdgeDelta>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMoveDelta {
    pub node_id: String,
    pub before: Vec3,
    pub after: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeDelta {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub violation_before: bool,
    pub violation_after: bool,
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
pub type SourcePathResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync + 'static>;

/// NLプロンプトをCLIへ送り結果を受け取るコールバック
/// (prompt, selected_node) -> response_json (NlDispatchResult相当)
pub type DispatchNl =
    Arc<dyn Fn(&str, Option<&str>) -> Result<String, String> + Send + Sync + 'static>;
