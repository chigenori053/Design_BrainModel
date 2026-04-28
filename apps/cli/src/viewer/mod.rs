use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::analyze::project::UnifiedAnalyzeResult;
use crate::nl::r#loop::{LoopOrigin, LoopPromotable, PromotionError, RepairLoopContext};
use crate::nl::types::{ExecutionPlan, Operation};
use crate::refactor::{
    ApplyPreviewPlan, GitCommitPreview, PreviewDiff, PromoteResult, RefactorActionKind,
    RefactorCandidate, RefactorOperation, RefactorTarget, StructureEdge,
    TransactionExecutionPreview, TransactionPreview, TransactionResult,
    execute_transactional_safe_apply, generate_apply_preview_plan, generate_git_commit_preview,
    generate_mock_preview_diff, generate_transaction_execution_preview,
    generate_transaction_preview, promote_sandbox_to_workspace,
};
use crate::service::ModuleNode;
use crate::source_index::QualifiedModuleId;

pub mod benchmark;
pub mod exporter;
pub mod function_map;
pub mod gui_dispatch;
pub mod keymap;
pub mod launcher;
pub mod nl_dispatch;
pub mod replay;
pub mod session;

pub use benchmark::{BenchmarkCommandReport, benchmark_structure_replay};
pub use exporter::{export_structure_view, export_structure_view_from_plan};
pub use function_map::{ViewerAction, ViewerFunction, resolve_action, viewer_function_map};
pub use gui_dispatch::{GuiCommandSpec, dispatch_gui_action};
pub use launcher::{LaunchResult, launch_native_viewer};
pub use nl_dispatch::{NlContext, NlDispatchResult, dispatch_nl};
pub use replay::{
    ReplayCommandReport, TimelineCommandReport, export_demo_replay_assets, summarize_timeline,
};
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

pub trait ViewProjection {
    fn from_execution_plan(plan: &ExecutionPlan) -> Self;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiffView {
    pub plan_operation: String,
    pub target: Option<String>,
    pub summary: String,
}

fn operation_name(operation: &Operation) -> &'static str {
    match operation {
        Operation::Analyze => "analyze",
        Operation::Refactor => "refactor",
        Operation::Validate => "validate",
        Operation::Composite(_) => "composite",
        Operation::Apply => "apply",
        Operation::Rollback => "rollback",
        Operation::Reload => "reload",
        Operation::Repair => "repair",
        Operation::NoOp => "noop",
    }
}

impl ViewProjection for DiffView {
    fn from_execution_plan(plan: &ExecutionPlan) -> Self {
        Self {
            plan_operation: operation_name(&plan.operation).to_string(),
            target: plan.target.as_ref().map(|path| path.display().to_string()),
            summary: plan
                .args
                .query
                .clone()
                .unwrap_or_else(|| operation_name(&plan.operation).to_string()),
        }
    }
}

impl ViewProjection for StructureViewIR {
    fn from_execution_plan(plan: &ExecutionPlan) -> Self {
        let target = plan
            .target
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "workspace".to_string());
        let op = operation_name(&plan.operation).to_string();
        let node = ViewNode {
            id: format!("plan:{op}:{target}"),
            label: target.clone(),
            layer: 0,
            role: op.clone(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let scene_3d = Structure3DIr {
            graph: SemanticGraph3D {
                nodes: vec![Node3D {
                    id: node.id.clone(),
                    label: node.label.clone(),
                    kind: node.role.clone(),
                    position: Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    size: 1.0,
                    importance: 1.0,
                    heat: 0.0,
                    source_binding: plan.target.as_ref().map(|path| SourceBinding {
                        file: path.clone(),
                        line_start: 1,
                        line_end: 1,
                        symbol: None,
                    }),
                }],
                edges: Vec::new(),
                clusters: vec![Cluster3D {
                    id: "plan".to_string(),
                    label: op.clone(),
                    nodes: vec![node.id.clone()],
                    color: "#4f7cff".to_string(),
                }],
                layers: Vec::new(),
            },
            runtime_paths: Vec::new(),
            overlays: ViewerOverlays3D::default(),
            timeline: Timeline3D::default(),
            camera: CameraPreset3D {
                focus_cluster: Some(node.id.clone()),
                mode: CameraMode::Architectural,
            },
        };
        StructureViewIR {
            nodes: vec![node],
            scene_3d: Some(scene_3d),
            history: vec![HistoryEntry {
                snapshot_index: 0,
                action: op,
                confidence: "1.00".to_string(),
            }],
            ..StructureViewIR::default()
        }
    }
}

impl LoopPromotable for StructureViewIR {
    fn promote(self) -> anyhow::Result<RepairLoopContext> {
        let logical_node = self.selection.selected_nodes.first().cloned();
        let scene = self
            .scene_3d
            .ok_or(PromotionError::NonUniqueSourceBinding)?;

        let mut bound_files = self
            .selection
            .selected_nodes
            .iter()
            .filter_map(|selected| {
                scene
                    .graph
                    .nodes
                    .iter()
                    .find(|node| &node.id == selected)
                    .and_then(|node| node.source_binding.as_ref())
                    .map(|binding| binding.file.clone())
            })
            .collect::<Vec<_>>();

        bound_files.sort();
        bound_files.dedup();

        if bound_files.len() != 1 {
            return Err(PromotionError::NonUniqueSourceBinding.into());
        }

        Ok(RepairLoopContext {
            target: bound_files.first().cloned(),
            logical_node,
            changed_files: Vec::new(),
            diagnostics: Vec::new(),
            rollback_token: None,
            previous_strategy: None,
            origin: LoopOrigin::Structure,
        })
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

impl StructureViewIR {
    pub(crate) fn to_core(&self) -> viewer_core::model::StructureViewIR {
        viewer_core::model::StructureViewIR {
            version: self.version,
            nodes: self
                .nodes
                .iter()
                .cloned()
                .map(ViewNode::into_core)
                .collect(),
            edges: self
                .edges
                .iter()
                .cloned()
                .map(ViewEdge::into_core)
                .collect(),
            preview: self.preview.clone().map(PreviewDiff::into_core),
            apply_preview: self.apply_preview.clone().map(ApplyPreviewPlan::into_core),
            transaction_preview: self
                .transaction_preview
                .clone()
                .map(TransactionPreview::into_core),
            transaction_execution: self
                .transaction_execution
                .clone()
                .map(TransactionExecutionPreview::into_core),
            transaction_result: self
                .transaction_result
                .clone()
                .map(TransactionResult::into_core),
            promote_result: self.promote_result.clone().map(PromoteResult::into_core),
            git_commit_preview: self
                .git_commit_preview
                .clone()
                .map(GitCommitPreview::into_core),
            snapshots: self
                .snapshots
                .iter()
                .cloned()
                .map(StructureSnapshot::into_core)
                .collect(),
            history: self
                .history
                .iter()
                .cloned()
                .map(|item| viewer_core::model::HistoryEntry {
                    snapshot_index: item.snapshot_index,
                    action: item.action,
                    confidence: item.confidence,
                })
                .collect(),
            risk_overlay: self
                .risk_overlay
                .iter()
                .cloned()
                .map(|item| viewer_core::model::RiskOverlay {
                    target: item.target,
                    level: item.level,
                    message: item.message,
                })
                .collect(),
            selection: viewer_core::model::ViewerSelection {
                selected_nodes: self.selection.selected_nodes.clone(),
                selected_edges: self
                    .selection
                    .selected_edges
                    .iter()
                    .map(|edge| viewer_core::model::StructureEdge {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                    })
                    .collect(),
                selection_mode: self.selection.selection_mode.clone(),
            },
            candidates: self
                .candidates
                .iter()
                .cloned()
                .map(|item| viewer_core::model::RefactorCandidate {
                    kind: match item.kind {
                        crate::refactor::RefactorActionKind::ExtractInterface => {
                            viewer_core::model::RefactorActionKind::ExtractInterface
                        }
                        crate::refactor::RefactorActionKind::RemoveDependency => {
                            viewer_core::model::RefactorActionKind::RemoveDependency
                        }
                        crate::refactor::RefactorActionKind::SplitModule => {
                            viewer_core::model::RefactorActionKind::SplitModule
                        }
                        crate::refactor::RefactorActionKind::MergeModule => {
                            viewer_core::model::RefactorActionKind::MergeModule
                        }
                        crate::refactor::RefactorActionKind::MoveFile => {
                            viewer_core::model::RefactorActionKind::MoveFile
                        }
                        crate::refactor::RefactorActionKind::RenameBoundary => {
                            viewer_core::model::RefactorActionKind::RenameBoundary
                        }
                        crate::refactor::RefactorActionKind::IntroduceService => {
                            viewer_core::model::RefactorActionKind::IntroduceService
                        }
                    },
                    title: item.title,
                    rationale: item.rationale,
                    confidence_milli: item.confidence_milli,
                    from_node: viewer_core::model::ModuleNode {
                        qualified_id: viewer_core::model::QualifiedModuleId {
                            crate_name: item.from_node.qualified_id.crate_name,
                            module_path: item.from_node.qualified_id.module_path,
                        },
                        logical_name: item.from_node.logical_name,
                        source_path: item.from_node.source_path,
                    },
                    to_node: viewer_core::model::ModuleNode {
                        qualified_id: viewer_core::model::QualifiedModuleId {
                            crate_name: item.to_node.qualified_id.crate_name,
                            module_path: item.to_node.qualified_id.module_path,
                        },
                        logical_name: item.to_node.logical_name,
                        source_path: item.to_node.source_path,
                    },
                })
                .collect(),
            heatmap: self
                .heatmap
                .iter()
                .cloned()
                .map(|item| viewer_core::model::HeatmapDelta {
                    target: item.target,
                    color: item.color,
                    label: item.label,
                    magnitude: item.magnitude,
                })
                .collect(),
            design_sync: viewer_core::model::DesignSyncStatus {
                design_md_updated: self.design_sync.design_md_updated,
                report_md_updated: self.design_sync.report_md_updated,
                ir_updated: self.design_sync.ir_updated,
                last_delta: self.design_sync.last_delta.clone(),
            },
            scene_3d: self.scene_3d.clone().map(Structure3DIr::into_core),
        }
    }
}

impl ViewNode {
    fn into_core(self) -> viewer_core::model::ViewNode {
        viewer_core::model::ViewNode {
            id: self.id,
            label: self.label,
            layer: self.layer,
            role: self.role,
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl ViewEdge {
    fn into_core(self) -> viewer_core::model::ViewEdge {
        viewer_core::model::ViewEdge {
            from: self.from,
            to: self.to,
            kind: self.kind,
            cycle: self.cycle,
        }
    }
}

impl PreviewDiff {
    fn into_core(self) -> viewer_core::model::PreviewDiff {
        viewer_core::model::PreviewDiff {
            candidate_id: self.candidate_id,
            summary: self.summary,
            estimated_effect: self.estimated_effect,
            safe: self.safe,
            diff_lines: self.diff_lines,
        }
    }
}

impl ApplyPreviewPlan {
    fn into_core(self) -> viewer_core::model::ApplyPreviewPlan {
        viewer_core::model::ApplyPreviewPlan {
            candidate_id: self.candidate_id,
            target_files: self.target_files,
            operations: self.operations,
            checks: self.checks,
            rollback: viewer_core::model::RollbackPreview {
                mode: self.rollback.mode,
                safe: self.rollback.safe,
            },
            write: self.write,
        }
    }
}

impl TransactionPreview {
    fn into_core(self) -> viewer_core::model::TransactionPreview {
        viewer_core::model::TransactionPreview {
            candidate_id: self.candidate_id,
            allowed: self.allowed,
            safe: self.safe,
            steps: self.steps,
            rollback_strategy: viewer_core::model::TransactionRollbackPreview {
                mode: self.rollback_strategy.mode,
                guaranteed: self.rollback_strategy.guaranteed,
            },
            write: self.write,
        }
    }
}

impl TransactionExecutionPreview {
    fn into_core(self) -> viewer_core::model::TransactionExecutionPreview {
        viewer_core::model::TransactionExecutionPreview {
            candidate_id: self.candidate_id,
            allowed: self.allowed,
            executed: self.executed,
            sandbox_write: viewer_core::model::SandboxWritePreview {
                enabled: self.sandbox_write.enabled,
                target_files: self.sandbox_write.target_files,
            },
            steps: self.steps,
            rollback_guaranteed: self.rollback_guaranteed,
            write: self.write,
        }
    }
}

impl TransactionResult {
    fn into_core(self) -> viewer_core::model::TransactionResult {
        viewer_core::model::TransactionResult {
            executed: self.executed,
            success: self.success,
            sandbox_root: self.sandbox_root,
            written_files: self.written_files,
            cargo_check: self.cargo_check,
            rollback_executed: self.rollback_executed,
        }
    }
}

impl PromoteResult {
    fn into_core(self) -> viewer_core::model::PromoteResult {
        viewer_core::model::PromoteResult {
            confirmed: self.confirmed,
            workspace_write: self.workspace_write,
            written_files: self.written_files,
            cargo_check: self.cargo_check,
            rollback_executed: self.rollback_executed,
        }
    }
}

impl GitCommitPreview {
    fn into_core(self) -> viewer_core::model::GitCommitPreview {
        viewer_core::model::GitCommitPreview {
            branch: self.branch,
            protected_branch: self.protected_branch,
            commit_allowed: self.commit_allowed,
            commit_message: self.commit_message,
            changed_files: self.changed_files,
            push: self.push,
        }
    }
}

impl StructureSnapshot {
    pub(crate) fn into_core(self) -> viewer_core::model::StructureSnapshot {
        viewer_core::model::StructureSnapshot {
            base: self.base.map(SnapshotGraph::into_core),
            delta: self.delta.into_core(),
            timestamp: self.timestamp,
            action: self.action,
            confidence: self.confidence,
        }
    }

    pub(crate) fn from_core(core: viewer_core::model::StructureSnapshot) -> Self {
        Self {
            base: core.base.map(SnapshotGraph::from_core),
            delta: SnapshotDelta::from_core(core.delta),
            timestamp: core.timestamp,
            action: core.action,
            confidence: core.confidence,
        }
    }
}

impl SnapshotGraph {
    fn into_core(self) -> viewer_core::model::SnapshotGraph {
        viewer_core::model::SnapshotGraph {
            nodes: self.nodes.into_iter().map(ViewNode::into_core).collect(),
            edges: self.edges.into_iter().map(ViewEdge::into_core).collect(),
        }
    }

    fn from_core(core: viewer_core::model::SnapshotGraph) -> Self {
        Self {
            nodes: core
                .nodes
                .into_iter()
                .map(|node| ViewNode {
                    id: node.id,
                    label: node.label,
                    layer: node.layer,
                    role: node.role,
                    x: node.x,
                    y: node.y,
                    z: node.z,
                })
                .collect(),
            edges: core
                .edges
                .into_iter()
                .map(|edge| ViewEdge {
                    from: edge.from,
                    to: edge.to,
                    kind: edge.kind,
                    cycle: edge.cycle,
                })
                .collect(),
        }
    }
}

impl SnapshotDelta {
    fn into_core(self) -> viewer_core::model::SnapshotDelta {
        viewer_core::model::SnapshotDelta {
            summary: self.summary,
            node_updates: self
                .node_updates
                .into_iter()
                .map(|item| viewer_core::model::NodeDelta {
                    id: item.id,
                    before: item.before.map(ViewNode::into_core),
                    after: item.after.map(ViewNode::into_core),
                })
                .collect(),
            edge_updates: self
                .edge_updates
                .into_iter()
                .map(|item| viewer_core::model::EdgeDeltaDelta {
                    from: item.from,
                    to: item.to,
                    kind: item.kind,
                    before: item.before.map(ViewEdge::into_core),
                    after: item.after.map(ViewEdge::into_core),
                })
                .collect(),
            overlay_updates: self
                .overlay_updates
                .into_iter()
                .map(|item| viewer_core::model::OverlayDelta {
                    target: item.target,
                    before: item.before.map(|overlay| viewer_core::model::RiskOverlay {
                        target: overlay.target,
                        level: overlay.level,
                        message: overlay.message,
                    }),
                    after: item.after.map(|overlay| viewer_core::model::RiskOverlay {
                        target: overlay.target,
                        level: overlay.level,
                        message: overlay.message,
                    }),
                })
                .collect(),
        }
    }

    fn from_core(core: viewer_core::model::SnapshotDelta) -> Self {
        Self {
            summary: core.summary,
            node_updates: core
                .node_updates
                .into_iter()
                .map(|item| NodeDelta {
                    id: item.id,
                    before: item.before.map(|node| ViewNode {
                        id: node.id,
                        label: node.label,
                        layer: node.layer,
                        role: node.role,
                        x: node.x,
                        y: node.y,
                        z: node.z,
                    }),
                    after: item.after.map(|node| ViewNode {
                        id: node.id,
                        label: node.label,
                        layer: node.layer,
                        role: node.role,
                        x: node.x,
                        y: node.y,
                        z: node.z,
                    }),
                })
                .collect(),
            edge_updates: core
                .edge_updates
                .into_iter()
                .map(|item| EdgeDeltaDelta {
                    from: item.from,
                    to: item.to,
                    kind: item.kind,
                    before: item.before.map(|edge| ViewEdge {
                        from: edge.from,
                        to: edge.to,
                        kind: edge.kind,
                        cycle: edge.cycle,
                    }),
                    after: item.after.map(|edge| ViewEdge {
                        from: edge.from,
                        to: edge.to,
                        kind: edge.kind,
                        cycle: edge.cycle,
                    }),
                })
                .collect(),
            overlay_updates: core
                .overlay_updates
                .into_iter()
                .map(|item| OverlayDelta {
                    target: item.target,
                    before: item.before.map(|overlay| RiskOverlay {
                        target: overlay.target,
                        level: overlay.level,
                        message: overlay.message,
                    }),
                    after: item.after.map(|overlay| RiskOverlay {
                        target: overlay.target,
                        level: overlay.level,
                        message: overlay.message,
                    }),
                })
                .collect(),
        }
    }
}

impl Structure3DIr {
    fn into_core(self) -> viewer_core::model::Structure3DIr {
        viewer_core::model::Structure3DIr {
            graph: viewer_core::model::SemanticGraph3D {
                nodes: self
                    .graph
                    .nodes
                    .into_iter()
                    .map(|node| viewer_core::model::Node3D {
                        id: node.id,
                        label: node.label,
                        kind: node.kind,
                        position: viewer_core::model::Vec3 {
                            x: node.position.x,
                            y: node.position.y,
                            z: node.position.z,
                        },
                        size: node.size,
                        importance: node.importance,
                        heat: node.heat,
                        source_binding: node.source_binding.map(|binding| {
                            viewer_core::model::SourceBinding {
                                file: binding.file,
                                line_start: binding.line_start,
                                line_end: binding.line_end,
                                symbol: binding.symbol,
                            }
                        }),
                    })
                    .collect(),
                edges: self
                    .graph
                    .edges
                    .into_iter()
                    .map(|edge| viewer_core::model::Edge3D {
                        from: edge.from,
                        to: edge.to,
                        weight: edge.weight,
                        edge_kind: edge.edge_kind,
                        violation: edge.violation,
                    })
                    .collect(),
                clusters: self
                    .graph
                    .clusters
                    .into_iter()
                    .map(|cluster| viewer_core::model::Cluster3D {
                        id: cluster.id,
                        label: cluster.label,
                        nodes: cluster.nodes,
                        color: cluster.color,
                    })
                    .collect(),
                layers: self
                    .graph
                    .layers
                    .into_iter()
                    .map(|layer| viewer_core::model::LayerPlane3D {
                        level: layer.level,
                        label: layer.label,
                        axis_x: layer.axis_x,
                        color: layer.color,
                    })
                    .collect(),
            },
            runtime_paths: self
                .runtime_paths
                .into_iter()
                .map(|path| viewer_core::model::RuntimePath3D {
                    id: path.id,
                    points: path
                        .points
                        .into_iter()
                        .map(|point| viewer_core::model::Vec3 {
                            x: point.x,
                            y: point.y,
                            z: point.z,
                        })
                        .collect(),
                    path_kind: match path.path_kind {
                        RuntimePathKind::Execution => {
                            viewer_core::model::RuntimePathKind::Execution
                        }
                        RuntimePathKind::Validation => {
                            viewer_core::model::RuntimePathKind::Validation
                        }
                        RuntimePathKind::Rollback => viewer_core::model::RuntimePathKind::Rollback,
                        RuntimePathKind::MemoryRelease => {
                            viewer_core::model::RuntimePathKind::MemoryRelease
                        }
                        RuntimePathKind::RefactorPreview => {
                            viewer_core::model::RuntimePathKind::RefactorPreview
                        }
                    },
                    animated: path.animated,
                })
                .collect(),
            overlays: viewer_core::model::ViewerOverlays3D {
                refactor: self.overlays.refactor.map(|overlay| {
                    viewer_core::model::RefactorOverlay3D {
                        selected_nodes: overlay.selected_nodes,
                        candidate_moves: overlay
                            .candidate_moves
                            .into_iter()
                            .map(|item| viewer_core::model::CandidateMove3D {
                                node_id: item.node_id,
                                from: viewer_core::model::Vec3 {
                                    x: item.from.x,
                                    y: item.from.y,
                                    z: item.from.z,
                                },
                                to: viewer_core::model::Vec3 {
                                    x: item.to.x,
                                    y: item.to.y,
                                    z: item.to.z,
                                },
                                reason: item.reason,
                            })
                            .collect(),
                        predicted_cycle_reduction: overlay.predicted_cycle_reduction,
                    }
                }),
                telemetry: self.overlays.telemetry.map(|item| {
                    viewer_core::model::TelemetryOverlay3D {
                        hot_path_count: item.hot_path_count,
                        rollback_count: item.rollback_count,
                        memory_release_count: item.memory_release_count,
                    }
                }),
                source_jump: self.overlays.source_jump,
                design_sync: self.overlays.design_sync,
            },
            timeline: viewer_core::model::Timeline3D {
                snapshots: self
                    .timeline
                    .snapshots
                    .into_iter()
                    .map(|snapshot| viewer_core::model::GraphSnapshot3D {
                        label: snapshot.label,
                        tick: snapshot.tick,
                        animation: viewer_core::model::GraphDeltaAnimation {
                            moved_nodes: snapshot
                                .animation
                                .moved_nodes
                                .into_iter()
                                .map(|item| viewer_core::model::NodeMoveDelta {
                                    node_id: item.node_id,
                                    before: viewer_core::model::Vec3 {
                                        x: item.before.x,
                                        y: item.before.y,
                                        z: item.before.z,
                                    },
                                    after: viewer_core::model::Vec3 {
                                        x: item.after.x,
                                        y: item.after.y,
                                        z: item.after.z,
                                    },
                                })
                                .collect(),
                            added_edges: snapshot
                                .animation
                                .added_edges
                                .into_iter()
                                .map(|edge| viewer_core::model::EdgeDelta {
                                    from: edge.from,
                                    to: edge.to,
                                    kind: edge.kind,
                                    violation_before: edge.violation_before,
                                    violation_after: edge.violation_after,
                                })
                                .collect(),
                            removed_edges: snapshot
                                .animation
                                .removed_edges
                                .into_iter()
                                .map(|edge| viewer_core::model::EdgeDelta {
                                    from: edge.from,
                                    to: edge.to,
                                    kind: edge.kind,
                                    violation_before: edge.violation_before,
                                    violation_after: edge.violation_after,
                                })
                                .collect(),
                            duration_ms: snapshot.animation.duration_ms,
                        },
                    })
                    .collect(),
                current_tick: self.timeline.current_tick,
                autoplay: self.timeline.autoplay,
            },
            camera: viewer_core::model::CameraPreset3D {
                focus_cluster: self.camera.focus_cluster,
                mode: match self.camera.mode {
                    CameraMode::Architectural => viewer_core::model::CameraMode::Architectural,
                    CameraMode::RuntimeFlow => viewer_core::model::CameraMode::RuntimeFlow,
                    CameraMode::RefactorPreview => viewer_core::model::CameraMode::RefactorPreview,
                },
            },
        }
    }
}

/// Parse `"RemoveDependency(<from> -> <to>)"` into `Some((from, to))`.
/// Returns `None` for any other format, including malformed input.
/// No regex — uses strip_prefix / strip_suffix / split_once only.
fn parse_remove_dependency(action: &str) -> Option<(String, String)> {
    let inner = action
        .strip_prefix("RemoveDependency(")?
        .strip_suffix(')')?;
    let (from, to) = inner.split_once(" -> ")?;
    let from = from.trim().to_string();
    let to = to.trim().to_string();
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some((from, to))
}

/// Inject a `RefactorCandidate` derived from `analysis.decision` into `ir.candidates`.
///
/// Only handles `RemoveDependency(<from> -> <to>)` actions in Phase1 Step2.
/// On parse failure the function is a no-op (panic-free, per spec).
/// Also marks any matching `ViewEdge` as `cycle = true`.
pub fn inject_recommendation_candidates(ir: &mut StructureViewIR, analysis: &UnifiedAnalyzeResult) {
    let Some((from, to)) = parse_remove_dependency(&analysis.decision.action) else {
        return;
    };

    let id = format!("cut-{}-{}", from, to);
    let qid_from = QualifiedModuleId {
        crate_name: String::new(),
        module_path: from.clone(),
    };
    let qid_to = QualifiedModuleId {
        crate_name: String::new(),
        module_path: to.clone(),
    };
    let confidence_milli =
        (analysis.decision.confidence * 1000.0).clamp(0.0, f64::from(u16::MAX)) as u16;
    let candidate = RefactorCandidate {
        candidate_id: id,
        module_id: qid_from.clone(),
        logical_name: from.clone(),
        kind: RefactorActionKind::RemoveDependency,
        operation: RefactorOperation::RemoveDependency,
        title: format!("Remove dependency {} -> {}", from, to),
        rationale: analysis.decision.expected_impact.clone(),
        confidence_milli,
        confidence: analysis.decision.confidence as f32,
        from_node: ModuleNode {
            qualified_id: qid_from,
            logical_name: from.clone(),
            source_path: None,
        },
        to_node: ModuleNode {
            qualified_id: qid_to,
            logical_name: to.clone(),
            source_path: None,
        },
        patch_plan: RefactorTarget::RemoveDependency {
            from: from.clone(),
            to: to.clone(),
        },
        source_path: PathBuf::new(),
        preview_hash: String::new(),
        base_file_hash: String::new(),
        target_nodes: vec![from.clone(), to.clone()],
        target_edges: vec![StructureEdge {
            from: from.clone(),
            to: to.clone(),
        }],
        target: RefactorTarget::RemoveDependency {
            from: from.clone(),
            to: to.clone(),
        },
    };
    ir.candidates.push(candidate);

    // Sync cycle flag on matching edges (Req §6)
    for edge in &mut ir.edges {
        if edge.from == from && edge.to == to {
            edge.cycle = true;
        }
    }
}

pub fn resolve_selected_candidate(ir: &StructureViewIR) -> Option<&RefactorCandidate> {
    for selected in &ir.selection.selected_edges {
        if let Some(candidate) = ir.candidates.iter().find(|candidate| {
            candidate
                .target_edges
                .iter()
                .any(|edge| selected.from == edge.from && selected.to == edge.to)
        }) {
            return Some(candidate);
        }
    }

    for selected_node in &ir.selection.selected_nodes {
        if let Some(candidate) = ir.candidates.iter().find(|candidate| {
            candidate
                .target_nodes
                .iter()
                .any(|node| node == selected_node)
        }) {
            return Some(candidate);
        }
    }

    ir.candidates.first()
}

pub fn sync_preview_with_selection(ir: &mut StructureViewIR) {
    ir.preview = resolve_selected_candidate(ir).map(generate_mock_preview_diff);
}

pub fn sync_apply_preview_with_selection(ir: &mut StructureViewIR) {
    ir.apply_preview = generate_apply_preview_plan(ir);
}

pub fn sync_transaction_preview_with_selection(ir: &mut StructureViewIR) {
    ir.transaction_preview = ir
        .apply_preview
        .as_ref()
        .and_then(generate_transaction_preview);
}

pub fn sync_transaction_execution_with_selection(ir: &mut StructureViewIR) {
    ir.transaction_execution = ir
        .transaction_preview
        .as_ref()
        .zip(ir.apply_preview.as_ref())
        .and_then(|(tx, apply)| generate_transaction_execution_preview(tx, apply));
}

pub fn execute_transaction_for_ir(ir: &mut StructureViewIR) {
    ir.transaction_result = ir
        .transaction_execution
        .as_ref()
        .and_then(execute_transactional_safe_apply);
}

pub fn promote_transaction_for_ir(ir: &mut StructureViewIR, confirmed: bool) {
    ir.promote_result = ir
        .transaction_result
        .as_ref()
        .and_then(|tx| promote_sandbox_to_workspace(tx, confirmed));
}

pub fn sync_git_commit_preview_with_selection(ir: &mut StructureViewIR) {
    ir.git_commit_preview = ir
        .promote_result
        .as_ref()
        .and_then(generate_git_commit_preview);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        DiffView, Node3D, SemanticGraph3D, SourceBinding, Structure3DIr, StructureViewIR, Vec3,
        ViewerSelection,
    };
    use crate::nl::r#loop::{LoopEntryState, LoopOrigin, LoopPromotable};
    use crate::source_index::ModuleSourceIndex;
    use crate::viewer::{ViewProjection, benchmark_structure_replay, export_demo_replay_assets};

    #[test]
    fn source_jump_exact_line() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("viewer_exact_line_{unique}"));
        fs::create_dir_all(root.join("src/runtime")).expect("runtime dir");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"viewer_exact_line\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "// banner\n\npub fn check() {}\n",
        )
        .expect("file");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let binding = index.exact_binding(&root, "determinism").expect("binding");
        assert_eq!(binding.file, PathBuf::from("src/runtime/determinism.rs"));
        assert_eq!(binding.line_start, 3);
        assert_eq!(binding.symbol.as_deref(), Some("check"));
    }

    #[test]
    fn benchmark_is_deterministic() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("viewer_benchmark_{unique}"));
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"viewer_benchmark\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("lib");
        fs::write(root.join("src/renderer.rs"), "use crate::debug;\n").expect("renderer");
        fs::write(root.join("src/debug.rs"), "use crate::renderer;\n").expect("debug");

        let first = benchmark_structure_replay(&root).expect("first");
        let second = benchmark_structure_replay(&root).expect("second");
        assert_eq!(first, second);
    }

    #[test]
    fn exports_demo_replay_assets() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("viewer_export_demo_{unique}"));
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"viewer_export_demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("lib");
        fs::write(root.join("src/renderer.rs"), "use crate::debug;\n").expect("renderer");
        fs::write(root.join("src/debug.rs"), "use crate::renderer;\n").expect("debug");

        let manifest = export_demo_replay_assets(&root, None).expect("manifest");
        assert!(root.join(".dbm/replay/scene_3d.json").exists());
        assert!(root.join(".dbm/replay/timeline.delta").exists());
        assert!(manifest.files.iter().any(|file| file == "benchmark.json"));
    }

    #[test]
    fn structure_selection_with_unique_binding_promotes_to_analyze() {
        let context = StructureViewIR {
            selection: ViewerSelection {
                selected_nodes: vec![String::from("determinism")],
                selected_edges: Vec::new(),
                selection_mode: "node".to_string(),
            },
            scene_3d: Some(Structure3DIr {
                graph: SemanticGraph3D {
                    nodes: vec![Node3D {
                        id: "determinism".to_string(),
                        label: "determinism".to_string(),
                        kind: "module".to_string(),
                        position: Vec3::default(),
                        size: 1.0,
                        importance: 1.0,
                        heat: 0.1,
                        source_binding: Some(SourceBinding {
                            file: PathBuf::from("src/runtime/determinism.rs"),
                            line_start: 1,
                            line_end: 4,
                            symbol: Some("check".to_string()),
                        }),
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        }
        .promote()
        .expect("unique binding should promote");
        assert_eq!(context.origin, LoopOrigin::Structure);
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::Analyze
        );
        assert_eq!(
            context.target,
            Some(PathBuf::from("src/runtime/determinism.rs"))
        );
    }

    #[test]
    fn structure_selection_with_multiple_bindings_is_rejected() {
        let error = StructureViewIR {
            selection: ViewerSelection {
                selected_nodes: vec![String::from("a"), String::from("b")],
                selected_edges: Vec::new(),
                selection_mode: "node".to_string(),
            },
            scene_3d: Some(Structure3DIr {
                graph: SemanticGraph3D {
                    nodes: vec![
                        Node3D {
                            id: "a".to_string(),
                            label: "a".to_string(),
                            kind: "module".to_string(),
                            position: Vec3::default(),
                            size: 1.0,
                            importance: 1.0,
                            heat: 0.1,
                            source_binding: Some(SourceBinding {
                                file: PathBuf::from("src/a.rs"),
                                line_start: 1,
                                line_end: 2,
                                symbol: None,
                            }),
                        },
                        Node3D {
                            id: "b".to_string(),
                            label: "b".to_string(),
                            kind: "module".to_string(),
                            position: Vec3::default(),
                            size: 1.0,
                            importance: 1.0,
                            heat: 0.1,
                            source_binding: Some(SourceBinding {
                                file: PathBuf::from("src/b.rs"),
                                line_start: 1,
                                line_end: 2,
                                symbol: None,
                            }),
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        }
        .promote()
        .expect_err("multiple source bindings must fail");
        assert!(error.to_string().contains("non-unique source binding"));
    }

    #[test]
    fn structure_and_diff_views_project_from_execution_plan() {
        use crate::nl::types::{ExecutionPlan, Operation, PlanSource};

        let plan = ExecutionPlan::new(
            Operation::Validate,
            Some(PathBuf::from("apps/cli/src/lib.rs")),
            PlanSource::System,
        );

        let ir = StructureViewIR::from_execution_plan(&plan);
        let diff = DiffView::from_execution_plan(&plan);

        assert_eq!(ir.nodes.len(), 1);
        assert!(ir.scene_3d.is_some());
        assert_eq!(diff.plan_operation, "validate");
        assert_eq!(diff.target.as_deref(), Some("apps/cli/src/lib.rs"));
    }
}
