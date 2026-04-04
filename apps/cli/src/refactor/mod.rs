use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};

use integration_layer::{
    CodePatch, MetricsDelta, PhaseType, PlanSummary, RefactorPhase,
    RefactorPlan as IntegrationRefactorPlan, RefactorPlanAction,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::coding::{ChangeSummary, ChangeType, CodeChange, CodeChangeSet};
use crate::service::{AnalysisDependency, AnalysisReport, ModuleNode, analyze_path};
use crate::source_index::{ModuleSourceIndex, QualifiedModuleId};

pub mod gui_bridge;
pub mod planner;
pub mod preview;
pub mod rollback;
pub mod runtime;
pub mod validator;

pub use gui_bridge::{
    GuiAction, GuiActionMode, build_refactor_candidates, gui_event_to_plan,
    gui_event_to_plan_with_candidates,
};
pub use planner::{PatchScope, create_refactor_plan, resolve_target};
pub use preview::{RefactorPreview, render_preview};
pub use rollback::{WorkspaceSnapshot, rollback_apply, snapshot_workspace};
pub use runtime::{
    RefactorApplyReport, RefactorRuntimeOptions, apply_refactor, build_apply_report,
};
pub use validator::{ValidationResult, validate_refactor};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StructureGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<StructureEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefactorTarget {
    Cycle,
    ExtractInterface { from: String, to: String },
    RemoveDependency { from: String, to: String },
    ModuleSplit(String),
    MergeModule(Vec<String>),
    LayerViolation(String),
    RenameBoundary(String),
    IntroduceService(String),
    FileMove(PathBuf),
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
pub enum RefactorOperation {
    ExtractInterface,
    RemoveDependency,
    SplitModule,
    MergeModule,
    MoveFile,
    RenameBoundary,
    IntroduceService,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefactorCandidate {
    pub candidate_id: String,
    pub module_id: QualifiedModuleId,
    pub logical_name: String,
    pub kind: RefactorActionKind,
    pub operation: RefactorOperation,
    pub title: String,
    pub rationale: String,
    pub confidence_milli: u16,
    pub confidence: f32,
    pub from_node: ModuleNode,
    pub to_node: ModuleNode,
    pub patch_plan: RefactorTarget,
    pub source_path: PathBuf,
    pub preview_hash: String,
    pub base_file_hash: String,
    pub target_nodes: Vec<String>,
    pub target_edges: Vec<StructureEdge>,
    pub target: RefactorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyResolverError {
    MissingSnapshot,
    MissingSourcePath,
    FileDriftDetected,
    PathOutsideWorkspace,
    PermissionDenied,
    TransactionRollback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefactorPlan {
    pub target: RefactorTarget,
    pub affected_files: Vec<PathBuf>,
    pub before_graph: StructureGraph,
    pub after_graph: StructureGraph,
    pub confidence: f32,
    pub root: PathBuf,
    pub removed_edges: Vec<StructureEdge>,
    pub moved_files: Vec<(PathBuf, PathBuf)>,
    pub estimated_delta: MetricsDelta,
    pub patches: Vec<CodePatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyResult {
    pub applied: bool,
    pub build_ok: bool,
    pub rolled_back: bool,
    pub changed_files: Vec<PathBuf>,
    pub commit_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefactorPreviewReport {
    pub root: String,
    pub plan: RefactorPlan,
    pub preview: RefactorPreview,
    pub validation: ValidationResult,
}

pub fn preview_report(
    root: &Path,
    target: Option<RefactorTarget>,
) -> Result<RefactorPreviewReport, String> {
    let analysis = analyze_path(root)?;
    let target = target.unwrap_or_else(|| planner::default_target(&analysis));
    let plan = create_refactor_plan(&analysis, target)?;
    let preview = render_preview(&plan);
    let validation = validate_refactor(&plan)?;
    Ok(RefactorPreviewReport {
        root: root.display().to_string(),
        plan,
        preview,
        validation,
    })
}

pub(crate) fn graph_from_analysis(report: &AnalysisReport) -> StructureGraph {
    let mut nodes = BTreeSet::new();
    let mut edges = report
        .dependencies
        .iter()
        .map(|dependency| {
            nodes.insert(dependency.from.clone());
            nodes.insert(dependency.to.clone());
            StructureEdge {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
            }
        })
        .collect::<Vec<_>>();
    if edges.is_empty() {
        let inferred = infer_edges_from_source(Path::new(&report.root));
        for edge in inferred {
            nodes.insert(edge.from.clone());
            nodes.insert(edge.to.clone());
            edges.push(edge);
        }
    }
    edges.sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
    StructureGraph {
        nodes: nodes.into_iter().collect(),
        edges,
    }
}

fn infer_edges_from_source(root: &Path) -> Vec<StructureEdge> {
    let src = root.join("src");
    let Ok(entries) = fs::read_dir(&src) else {
        return Vec::new();
    };
    let mut edges = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let Some(from) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            let Some(remainder) = trimmed.strip_prefix("use crate::") else {
                continue;
            };
            let to = remainder
                .split([':', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            if !to.is_empty() {
                edges.push(StructureEdge {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }
    }
    edges.sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
    edges.dedup();
    edges
}

pub(crate) fn counts_by_node(dependencies: &[AnalysisDependency]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for dependency in dependencies {
        *counts.entry(dependency.from.clone()).or_insert(0) += 1;
        *counts.entry(dependency.to.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn source_index_for_report(report: &AnalysisReport) -> ModuleSourceIndex {
    ModuleSourceIndex::build(Path::new(&report.root)).unwrap_or_default()
}

pub fn candidate_snapshot_dir(root: &Path) -> PathBuf {
    root.join(".dbm").join("refactor").join("candidates")
}

pub fn candidate_snapshot_path(root: &Path, candidate_id: &str) -> PathBuf {
    candidate_snapshot_dir(root).join(format!("{candidate_id}.json"))
}

pub fn resolve_preview_candidate(
    root: &Path,
    module_id: &QualifiedModuleId,
    logical_name: &str,
    source_index: &ModuleSourceIndex,
) -> Result<PathBuf, String> {
    let qualified = format!("{}::{}", module_id.crate_name, module_id.module_path);
    let path = source_index
        .resolve(&qualified)
        .ok()
        .flatten()
        .or_else(|| source_index.resolve(logical_name).ok().flatten())
        .or_else(|| {
            source_index
                .bind_graph_node(logical_name)
                .map(|(_, path)| path)
        })
        .ok_or_else(|| format!("unable to resolve source path for module {logical_name}"))?;
    let absolute = workspace_absolute_path(root, &path).map_err(|_| {
        format!(
            "resolved path escapes workspace boundary: {}",
            path.display()
        )
    })?;
    if !absolute.exists() {
        return Err(format!(
            "resolved path does not exist: {}",
            absolute.display()
        ));
    }
    let readonly = fs::metadata(&absolute)
        .map_err(|err| format!("failed to inspect {}: {err}", absolute.display()))?
        .permissions()
        .readonly();
    if readonly {
        return Err(format!(
            "write permission denied for {}",
            absolute.display()
        ));
    }
    Ok(path)
}

fn workspace_absolute_path(root: &Path, path: &Path) -> Result<PathBuf, ApplyResolverError> {
    if path.as_os_str().is_empty() {
        return Err(ApplyResolverError::MissingSourcePath);
    }
    if path.is_absolute() {
        return if path.starts_with(root) {
            Ok(path.to_path_buf())
        } else {
            Err(ApplyResolverError::PathOutsideWorkspace)
        };
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(ApplyResolverError::PathOutsideWorkspace);
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(ApplyResolverError::PathOutsideWorkspace);
            }
        }
    }

    Ok(root.join(normalized))
}

pub(crate) fn resolve_candidate_source_path(
    report: &AnalysisReport,
    modules: &[String],
) -> PathBuf {
    modules
        .iter()
        .find_map(|module| {
            report
                .graph_nodes
                .iter()
                .find(|node| node.logical_name == *module)
                .and_then(|node| node.source_path.clone())
        })
        .or_else(|| source_index_for_report(report).all_paths().first().cloned())
        .unwrap_or_default()
}

pub fn candidate_from_module(
    report: &AnalysisReport,
    logical_name: &str,
    kind: RefactorActionKind,
    title: String,
    rationale: String,
    target: RefactorTarget,
    target_nodes: Vec<String>,
    target_edges: Vec<StructureEdge>,
    confidence_milli: u16,
) -> RefactorCandidate {
    let from_node = resolve_module_node(report, logical_name);
    let to_node = from_node.clone();
    let source_index = source_index_for_report(report);
    let source_path = resolve_preview_candidate(
        Path::new(&report.root),
        &from_node.qualified_id,
        logical_name,
        &source_index,
    )
    .unwrap_or_else(|_| resolve_candidate_source_path(report, &target_nodes));
    let operation = operation_from_kind(&kind);
    let base_file_hash = hash_file(Path::new(&report.root), &source_path).unwrap_or_default();
    let preview_hash = preview_hash(&source_path, &operation, &base_file_hash);
    let confidence = f32::from(confidence_milli) / 1000.0;
    RefactorCandidate {
        candidate_id: candidate_id(
            &from_node.qualified_id,
            logical_name,
            &operation,
            &source_path,
        ),
        module_id: from_node.qualified_id.clone(),
        logical_name: logical_name.to_string(),
        kind,
        operation,
        title,
        rationale,
        confidence_milli,
        confidence,
        from_node,
        to_node,
        patch_plan: target.clone(),
        source_path,
        preview_hash,
        base_file_hash,
        target_nodes,
        target_edges,
        target,
    }
}

pub fn persist_refactor_candidates(
    root: &Path,
    candidates: &[RefactorCandidate],
) -> Result<(), String> {
    let dir = candidate_snapshot_dir(root);
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    for candidate in candidates {
        let path = candidate_snapshot_path(root, &candidate.candidate_id);
        fs::write(
            &path,
            serde_json::to_string_pretty(candidate).map_err(|err| err.to_string())?,
        )
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

pub fn candidate_for_target(report: &AnalysisReport, target: &RefactorTarget) -> RefactorCandidate {
    match target {
        RefactorTarget::Cycle => candidate_from_module(
            report,
            report
                .graph_nodes
                .first()
                .map(|node| node.logical_name.as_str())
                .unwrap_or("core"),
            RefactorActionKind::ExtractInterface,
            "Cycle remediation".to_string(),
            "Break the dominant cycle via interface extraction".to_string(),
            target.clone(),
            Vec::new(),
            Vec::new(),
            900,
        ),
        RefactorTarget::ExtractInterface { from, to } => candidate_from_module(
            report,
            from,
            RefactorActionKind::ExtractInterface,
            format!("Extract interface {from} -> {to}"),
            "Break direct dependency and route through an interface boundary".to_string(),
            target.clone(),
            vec![from.clone(), to.clone()],
            vec![StructureEdge {
                from: from.clone(),
                to: to.clone(),
            }],
            910,
        ),
        RefactorTarget::RemoveDependency { from, to } => candidate_from_module(
            report,
            from,
            RefactorActionKind::RemoveDependency,
            format!("Remove dependency {from} -> {to}"),
            "Reduce coupling on the selected dependency path".to_string(),
            target.clone(),
            vec![from.clone(), to.clone()],
            vec![StructureEdge {
                from: from.clone(),
                to: to.clone(),
            }],
            760,
        ),
        RefactorTarget::ModuleSplit(module) => candidate_from_module(
            report,
            module,
            RefactorActionKind::SplitModule,
            format!("Split module {module}"),
            "Partition a dense cluster into smaller responsibilities".to_string(),
            target.clone(),
            vec![module.clone()],
            Vec::new(),
            820,
        ),
        RefactorTarget::MergeModule(modules) => candidate_from_module(
            report,
            modules.first().map(String::as_str).unwrap_or("core"),
            RefactorActionKind::MergeModule,
            format!("Merge {}", modules.join(" + ")),
            "Collapse tightly coupled nodes into a single boundary".to_string(),
            target.clone(),
            modules.clone(),
            Vec::new(),
            660,
        ),
        RefactorTarget::LayerViolation(detail) => candidate_from_module(
            report,
            detail.split("->").next().unwrap_or("core").trim(),
            RefactorActionKind::RemoveDependency,
            format!("Fix layer violation {detail}"),
            "Stabilize layer direction before apply".to_string(),
            target.clone(),
            vec![detail.clone()],
            Vec::new(),
            780,
        ),
        RefactorTarget::RenameBoundary(module) => candidate_from_module(
            report,
            module,
            RefactorActionKind::RenameBoundary,
            format!("Rename boundary {module}"),
            "Clarify boundary naming".to_string(),
            target.clone(),
            vec![module.clone()],
            Vec::new(),
            700,
        ),
        RefactorTarget::IntroduceService(module) => candidate_from_module(
            report,
            module,
            RefactorActionKind::IntroduceService,
            format!("Introduce service for {module}"),
            "Move orchestration into a service node".to_string(),
            target.clone(),
            vec![module.clone()],
            Vec::new(),
            730,
        ),
        RefactorTarget::FileMove(path) => candidate_from_module(
            report,
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("moved_file"),
            RefactorActionKind::MoveFile,
            format!("Move file {}", path.display()),
            "Move file to a new boundary".to_string(),
            target.clone(),
            vec![path.display().to_string()],
            Vec::new(),
            650,
        ),
    }
}

pub fn load_refactor_candidate(
    root: &Path,
    candidate_id: &str,
) -> Result<RefactorCandidate, ApplyResolverError> {
    let path = candidate_snapshot_path(root, candidate_id);
    let raw = fs::read_to_string(&path).map_err(|_| ApplyResolverError::MissingSnapshot)?;
    serde_json::from_str(&raw).map_err(|_| ApplyResolverError::MissingSnapshot)
}

pub fn load_matching_refactor_candidate(
    root: &Path,
    logical_name: &str,
    operation: RefactorOperation,
) -> Result<RefactorCandidate, ApplyResolverError> {
    let dir = candidate_snapshot_dir(root);
    let entries = fs::read_dir(&dir).map_err(|_| ApplyResolverError::MissingSnapshot)?;
    let mut matches = entries
        .flatten()
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|raw| serde_json::from_str::<RefactorCandidate>(&raw).ok())
        .filter(|candidate| {
            candidate.logical_name == logical_name && candidate.operation == operation
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    matches.pop().ok_or(ApplyResolverError::MissingSnapshot)
}

pub fn validate_apply_candidate(
    root: &Path,
    candidate: &RefactorCandidate,
) -> Result<PathBuf, ApplyResolverError> {
    let absolute = workspace_absolute_path(root, &candidate.source_path)?;
    if !absolute.exists() {
        return Err(ApplyResolverError::MissingSourcePath);
    }
    let metadata = fs::metadata(&absolute).map_err(|_| ApplyResolverError::MissingSourcePath)?;
    if metadata.permissions().readonly() {
        return Err(ApplyResolverError::PermissionDenied);
    }
    let current =
        hash_file_absolute(&absolute).map_err(|_| ApplyResolverError::TransactionRollback)?;
    if current != candidate.base_file_hash {
        return Err(ApplyResolverError::FileDriftDetected);
    }
    Ok(absolute)
}

pub fn apply_resolver_error_message(error: &ApplyResolverError, path: Option<&Path>) -> String {
    match error {
        ApplyResolverError::MissingSnapshot => {
            "Apply failed: missing preview snapshot. Re-run refactor preview before apply."
                .to_string()
        }
        ApplyResolverError::MissingSourcePath => format!(
            "Apply failed: source path is missing{}",
            path.map(|p| format!(" on {}", p.display()))
                .unwrap_or_default()
        ),
        ApplyResolverError::FileDriftDetected => format!(
            "Apply failed: File drift detected on {}. Re-run refactor preview before apply.",
            path.map(|p| p.display().to_string())
                .unwrap_or_else(|| "target file".to_string())
        ),
        ApplyResolverError::PathOutsideWorkspace => {
            "Apply failed: target path escapes workspace boundary.".to_string()
        }
        ApplyResolverError::PermissionDenied => format!(
            "Apply failed: permission denied on {}",
            path.map(|p| p.display().to_string())
                .unwrap_or_else(|| "target file".to_string())
        ),
        ApplyResolverError::TransactionRollback => {
            "Apply failed: transactional rollback triggered before write.".to_string()
        }
    }
}

fn operation_from_kind(kind: &RefactorActionKind) -> RefactorOperation {
    match kind {
        RefactorActionKind::ExtractInterface => RefactorOperation::ExtractInterface,
        RefactorActionKind::RemoveDependency => RefactorOperation::RemoveDependency,
        RefactorActionKind::SplitModule => RefactorOperation::SplitModule,
        RefactorActionKind::MergeModule => RefactorOperation::MergeModule,
        RefactorActionKind::MoveFile => RefactorOperation::MoveFile,
        RefactorActionKind::RenameBoundary => RefactorOperation::RenameBoundary,
        RefactorActionKind::IntroduceService => RefactorOperation::IntroduceService,
    }
}

fn candidate_id(
    module_id: &QualifiedModuleId,
    logical_name: &str,
    operation: &RefactorOperation,
    source_path: &Path,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(module_id.crate_name.as_bytes());
    hasher.update(module_id.module_path.as_bytes());
    hasher.update(logical_name.as_bytes());
    hasher.update(format!("{operation:?}").as_bytes());
    hasher.update(source_path.display().to_string().as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

fn preview_hash(source_path: &Path, operation: &RefactorOperation, base_file_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_path.display().to_string().as_bytes());
    hasher.update(format!("{operation:?}").as_bytes());
    hasher.update(base_file_hash.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn hash_file(root: &Path, path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    hash_file_absolute(&absolute).map_err(|err| err.to_string())
}

fn hash_file_absolute(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn resolve_module_node(report: &AnalysisReport, logical_name: &str) -> ModuleNode {
    if let Some(node) = report
        .graph_nodes
        .iter()
        .find(|node| node.logical_name == logical_name)
    {
        return node.clone();
    }

    let index = source_index_for_report(report);
    if let Some((qualified_id, source_path)) = index.bind_graph_node(logical_name) {
        return ModuleNode {
            qualified_id,
            logical_name: logical_name.to_string(),
            source_path: Some(source_path),
        };
    }

    ModuleNode {
        qualified_id: crate::source_index::QualifiedModuleId {
            crate_name: Path::new(&report.root)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .replace('-', "_"),
            module_path: logical_name.replace('-', "_"),
        },
        logical_name: logical_name.to_string(),
        source_path: None,
    }
}

pub(crate) fn integration_plan_for_target(
    target: &RefactorTarget,
    detail: Option<&str>,
) -> IntegrationRefactorPlan {
    let action = match target {
        RefactorTarget::Cycle => {
            let mut parts = detail
                .unwrap_or("module:dependency")
                .split(':')
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if parts.len() < 2 {
                parts = vec!["module".to_string(), "dependency".to_string()];
            }
            RefactorPlanAction::IntroduceInterface {
                between: (parts[0].clone(), parts[1].clone()),
            }
        }
        RefactorTarget::ExtractInterface { from, to } => RefactorPlanAction::IntroduceInterface {
            between: (from.clone(), to.clone()),
        },
        RefactorTarget::RemoveDependency { from, to } => RefactorPlanAction::RemoveDependency {
            from: from.clone(),
            to: to.clone(),
        },
        RefactorTarget::ModuleSplit(module) => RefactorPlanAction::SplitModule {
            target: module.clone(),
        },
        RefactorTarget::MergeModule(modules) => RefactorPlanAction::ExtractComponent {
            from: modules
                .first()
                .cloned()
                .unwrap_or_else(|| "merged_module".to_string()),
        },
        RefactorTarget::LayerViolation(detail) => {
            let mut parts = detail
                .split("->")
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>();
            if parts.len() < 2 {
                parts = vec![detail.clone(), "interface".to_string()];
            }
            RefactorPlanAction::MoveDependency {
                from: parts[0].clone(),
                to: parts[1].clone(),
                via: Some(format!("{}_{}_interface", parts[0], parts[1])),
            }
        }
        RefactorTarget::RenameBoundary(module) => RefactorPlanAction::ExtractComponent {
            from: module.clone(),
        },
        RefactorTarget::IntroduceService(module) => RefactorPlanAction::ExtractComponent {
            from: module.clone(),
        },
        RefactorTarget::FileMove(path) => RefactorPlanAction::ExtractComponent {
            from: path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("moved_file")
                .to_string(),
        },
    };

    IntegrationRefactorPlan {
        phases: vec![RefactorPhase {
            phase_type: match target {
                RefactorTarget::Cycle | RefactorTarget::ExtractInterface { .. } => {
                    PhaseType::BreakCycle
                }
                RefactorTarget::ModuleSplit(_)
                | RefactorTarget::MergeModule(_)
                | RefactorTarget::RenameBoundary(_)
                | RefactorTarget::IntroduceService(_)
                | RefactorTarget::FileMove(_) => PhaseType::RestructureModules,
                RefactorTarget::RemoveDependency { .. } | RefactorTarget::LayerViolation(_) => {
                    PhaseType::FixLayering
                }
            },
            actions: vec![action],
        }],
        summary: PlanSummary {
            total_actions: 1,
            phase_count: 1,
            expected_improvement: MetricsDelta {
                cycle_count: 0,
                layer_violations: 0,
                coupling_score_milli: -100,
            },
        },
    }
}

pub(crate) fn file_move_change_set(root: &Path, source: &Path) -> Result<CodeChangeSet, String> {
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        root.join(source)
    };
    let bytes = fs::read_to_string(&source)
        .map_err(|err| format!("failed to read {}: {err}", source.display()))?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid source file name: {}", source.display()))?;
    let destination = PathBuf::from("src").join("moved").join(file_name);
    let relative_source = source
        .strip_prefix(root)
        .map_err(|_| format!("file move path escapes root: {}", source.display()))?
        .to_path_buf();
    Ok(CodeChangeSet {
        patches: vec![],
        changes: vec![
            CodeChange {
                file_path: destination.display().to_string(),
                change_type: ChangeType::CreateFile,
                hunks: vec![crate::coding::DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: bytes,
                }],
            },
            CodeChange {
                file_path: relative_source.display().to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![crate::coding::DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: String::new(),
                }],
            },
        ],
        summary: ChangeSummary {
            total_changes: 2,
            create_files: 1,
            modify_files: 1,
            move_files: 1,
        },
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use integration_layer::{Cycle, CycleReport, LayerModel};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("design_cli_refactor_{name}_{unique}"));
        fs::create_dir_all(dir.join("src")).expect("create src");
        dir
    }

    fn sample_analysis(root: &Path) -> AnalysisReport {
        AnalysisReport {
            root: root.display().to_string(),
            total_files: 3,
            source_files: 3,
            avg_complexity: "1.0".to_string(),
            manifests: vec!["Cargo.toml".to_string()],
            languages: BTreeMap::from([(String::from("Rust"), 3usize)]),
            top_level_entries: vec!["src".to_string()],
            architecture_hints: vec!["has-tests".to_string()],
            modules: vec![
                crate::service::AnalysisModule {
                    name: "renderer".to_string(),
                    file_count: 1,
                    source_path: "src/renderer.rs".to_string(),
                },
                crate::service::AnalysisModule {
                    name: "debug".to_string(),
                    file_count: 1,
                    source_path: "src/debug.rs".to_string(),
                },
            ],
            graph_nodes: vec![
                crate::service::ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: "design_cli_refactor_cycle_plan".to_string(),
                        module_path: "renderer".to_string(),
                    },
                    logical_name: "renderer".to_string(),
                    source_path: Some(PathBuf::from("src/renderer.rs")),
                },
                crate::service::ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: "design_cli_refactor_cycle_plan".to_string(),
                        module_path: "debug".to_string(),
                    },
                    logical_name: "debug".to_string(),
                    source_path: Some(PathBuf::from("src/debug.rs")),
                },
            ],
            dependencies: vec![
                crate::service::AnalysisDependency {
                    from: "renderer".to_string(),
                    to: "debug".to_string(),
                },
                crate::service::AnalysisDependency {
                    from: "debug".to_string(),
                    to: "renderer".to_string(),
                },
            ],
            todo_files: 0,
            cycles: CycleReport {
                has_cycle: true,
                cycles: vec![Cycle {
                    nodes: vec!["renderer".to_string(), "debug".to_string()],
                    size: 2,
                }],
            },
            layers: LayerModel { layers: Vec::new() },
            violations: Vec::new(),
            roles: Vec::new(),
            semantic_layers: Vec::new(),
            data_flow: Vec::new(),
            issues: Vec::new(),
            code_issues: Vec::new(),
            summary: crate::service::AnalysisSummary::default(),
            next_action: String::new(),
            root_cause: None,
            refactor_plan: Vec::new(),
        }
    }

    #[test]
    fn cycle_planner_creates_removed_edge_and_previewable_plan() {
        let root = temp_dir("cycle_plan");
        let report = sample_analysis(&root);
        let plan = create_refactor_plan(&report, RefactorTarget::Cycle).expect("plan");
        assert_eq!(plan.removed_edges.len(), 1);
        assert!(matches!(plan.target, RefactorTarget::Cycle));
        let preview = render_preview(&plan);
        assert!(preview.cli_text_preview.contains("Before:"));
        assert!(preview.removed_cycle_edge.is_some());
    }

    #[test]
    fn validator_rejects_unresolved_cycle() {
        let root = temp_dir("validator");
        let report = sample_analysis(&root);
        let mut plan = create_refactor_plan(&report, RefactorTarget::Cycle).expect("plan");
        plan.after_graph = plan.before_graph.clone();
        let validation = validate_refactor(&plan).expect("validation");
        assert!(!validation.valid);
        assert!(!validation.cycle_removed);
    }

    #[test]
    fn rollback_restores_snapshot() {
        let root = temp_dir("rollback");
        let file = root.join("src/lib.rs");
        fs::write(&file, "pub fn original() {}\n").expect("write original");
        let snapshot = snapshot_workspace(&root, &[PathBuf::from("src/lib.rs")]).expect("snapshot");
        fs::write(&file, "pub fn changed() {}\n").expect("write changed");
        rollback_apply(&snapshot).expect("rollback");
        assert_eq!(
            fs::read_to_string(&file).expect("read restored"),
            "pub fn original() {}\n"
        );
    }

    #[test]
    fn gui_bridge_generates_plan_from_click_event() {
        let root = temp_dir("gui_bridge");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"gui_bridge\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("write lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("write renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("write debug");

        let plan = gui_event_to_plan(GuiAction {
            action: "refactor".to_string(),
            target: "cycle".to_string(),
            node: Some("renderer".to_string()),
            project_root: Some(root),
            selected_nodes: Vec::new(),
            selected_edges: Vec::new(),
            mode: crate::refactor::GuiActionMode::Apply,
        })
        .expect("gui plan");
        assert!(matches!(plan.target, RefactorTarget::Cycle));
    }

    #[test]
    fn cycle_apply_uses_safe_runtime_and_succeeds() {
        let root = temp_dir("apply_cycle");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"apply_cycle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("write lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("write renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("write debug");
        let report = sample_analysis(&root);
        let plan = create_refactor_plan(&report, RefactorTarget::Cycle).expect("plan");
        let validation = validate_refactor(&plan).expect("validation");
        let apply = apply_refactor(
            &plan,
            &runtime::RefactorRuntimeOptions {
                auto_commit: false,
                no_build: true,
                backup: true,
                format: false,
            },
            &validation,
        )
        .expect("apply");
        assert!(apply.applied);
        assert!(apply.build_ok);
        assert!(!apply.rolled_back);
    }
}
