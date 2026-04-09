use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::service::{AnalysisReport, analyze_path};

use super::{
    RefactorActionKind, RefactorCandidate, RefactorPlan, RefactorTarget, StructureEdge,
    candidate_from_module, create_refactor_plan, persist_refactor_candidates,
    planner::default_target,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GuiActionMode {
    Preview,
    #[default]
    Apply,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAction {
    pub action: String,
    pub target: String,
    pub node: Option<String>,
    pub project_root: Option<PathBuf>,
    #[serde(default)]
    pub selected_nodes: Vec<String>,
    #[serde(default)]
    pub selected_edges: Vec<StructureEdge>,
    #[serde(default)]
    pub mode: GuiActionMode,
}

pub fn gui_event_to_plan(event: GuiAction) -> Result<RefactorPlan, String> {
    gui_event_to_plan_with_candidates(event).map(|(plan, _)| plan)
}

pub fn gui_event_to_plan_with_candidates(
    event: GuiAction,
) -> Result<(RefactorPlan, Vec<RefactorCandidate>), String> {
    if event.action != "refactor" {
        return Err(format!("unsupported GUI action: {}", event.action));
    }
    let root = event
        .project_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let analysis = analyze_path(&root)?;
    let candidates = build_refactor_candidates(&analysis, &event);
    let _ = persist_refactor_candidates(&root, &candidates);
    let target = target_from_event(&analysis, &event, &candidates);
    let plan = create_refactor_plan(&analysis, target)
        .or_else(|_| create_refactor_plan(&analysis, default_target(&analysis)))?;
    Ok((plan, candidates))
}

pub fn build_refactor_candidates(
    analysis: &AnalysisReport,
    event: &GuiAction,
) -> Vec<RefactorCandidate> {
    let mut candidates = Vec::new();
    let selected_nodes = if event.selected_nodes.is_empty() {
        event.node.clone().into_iter().collect()
    } else {
        event.selected_nodes.clone()
    };
    let selected_edges = event.selected_edges.clone();

    for edge in &selected_edges {
        candidates.push(candidate_from_module(
            analysis,
            &edge.from,
            RefactorActionKind::ExtractInterface,
            format!("Extract interface {} -> {}", edge.from, edge.to),
            "Break direct dependency and route through an interface boundary".to_string(),
            RefactorTarget::ExtractInterface {
                from: edge.from.clone(),
                to: edge.to.clone(),
            },
            vec![edge.from.clone(), edge.to.clone()],
            vec![edge.clone()],
            910,
        ));
        candidates.push(candidate_from_module(
            analysis,
            &edge.from,
            RefactorActionKind::RemoveDependency,
            format!("Remove dependency {} -> {}", edge.from, edge.to),
            "Reduce coupling on the selected dependency path".to_string(),
            RefactorTarget::RemoveDependency {
                from: edge.from.clone(),
                to: edge.to.clone(),
            },
            vec![edge.from.clone(), edge.to.clone()],
            vec![edge.clone()],
            760,
        ));
    }

    if let Some(primary) = selected_nodes.first() {
        candidates.push(candidate_from_module(
            analysis,
            primary,
            RefactorActionKind::SplitModule,
            format!("Split module {primary}"),
            "Partition a dense cluster into smaller responsibilities".to_string(),
            RefactorTarget::ModuleSplit(primary.clone()),
            vec![primary.clone()],
            Vec::new(),
            820,
        ));
        candidates.push(candidate_from_module(
            analysis,
            primary,
            RefactorActionKind::RenameBoundary,
            format!("Rename boundary {primary}"),
            "Clarify the edge-facing role of the selected module".to_string(),
            RefactorTarget::RenameBoundary(primary.clone()),
            vec![primary.clone()],
            Vec::new(),
            700,
        ));
        candidates.push(candidate_from_module(
            analysis,
            primary,
            RefactorActionKind::IntroduceService,
            format!("Introduce service for {primary}"),
            "Move orchestration into a service node without mutating the core node".to_string(),
            RefactorTarget::IntroduceService(primary.clone()),
            vec![primary.clone()],
            Vec::new(),
            730,
        ));
    }

    if selected_nodes.len() >= 2 {
        candidates.push(candidate_from_module(
            analysis,
            &selected_nodes[0],
            RefactorActionKind::MergeModule,
            format!("Merge {}", selected_nodes.join(" + ")),
            "Collapse tightly coupled nodes into a single boundary".to_string(),
            RefactorTarget::MergeModule(selected_nodes.clone()),
            selected_nodes.clone(),
            Vec::new(),
            660,
        ));
    }

    if candidates.is_empty() {
        candidates.push(default_candidate(analysis));
    }

    candidates.sort_by(|lhs, rhs| {
        rhs.confidence_milli
            .cmp(&lhs.confidence_milli)
            .then_with(|| lhs.title.cmp(&rhs.title))
    });
    candidates
}

fn target_from_event(
    analysis: &AnalysisReport,
    event: &GuiAction,
    candidates: &[RefactorCandidate],
) -> RefactorTarget {
    match event.target.as_str() {
        "auto" | "" => candidates
            .first()
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "cycle" | "Cycle" => RefactorTarget::Cycle,
        "extract-interface" | "ExtractInterface" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::ExtractInterface)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "remove-dependency" | "RemoveDependency" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::RemoveDependency)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "module-split" | "SplitModule" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::SplitModule)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "merge-module" | "MergeModule" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::MergeModule)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "rename-boundary" | "RenameBoundary" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::RenameBoundary)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        "introduce-service" | "IntroduceService" => candidates
            .iter()
            .find(|candidate| candidate.kind == RefactorActionKind::IntroduceService)
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| default_target(analysis)),
        other => super::resolve_target(analysis, Some(other), event.node.as_deref(), None),
    }
}

fn default_candidate(analysis: &AnalysisReport) -> RefactorCandidate {
    let target = default_target(analysis);
    let primary = analysis
        .graph_nodes
        .first()
        .map(|node| node.logical_name.clone())
        .unwrap_or_else(|| "core".to_string());
    candidate_from_module(
        analysis,
        &primary,
        RefactorActionKind::ExtractInterface,
        "Default cycle remediation".to_string(),
        "Fallback to the highest-priority architecture remediation".to_string(),
        target,
        Vec::new(),
        Vec::new(),
        640,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use integration_layer::{Cycle, CycleReport, LayerModel};

    use super::*;

    fn sample_project() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("gui_bridge_{unique}"));
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"gui_bridge\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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

    fn sample_analysis(root: &PathBuf) -> AnalysisReport {
        AnalysisReport {
            root: root.display().to_string(),
            total_files: 2,
            source_files: 2,
            avg_complexity: "1.0".to_string(),
            manifests: vec!["Cargo.toml".to_string()],
            languages: BTreeMap::new(),
            top_level_entries: vec!["src".to_string()],
            architecture_hints: vec![],
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
                        crate_name: "gui_bridge".to_string(),
                        module_path: "renderer".to_string(),
                    },
                    logical_name: "renderer".to_string(),
                    source_path: Some(PathBuf::from("src/renderer.rs")),
                },
                crate::service::ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: "gui_bridge".to_string(),
                        module_path: "debug".to_string(),
                    },
                    logical_name: "debug".to_string(),
                    source_path: Some(PathBuf::from("src/debug.rs")),
                },
            ],
            dependencies: vec![
                crate::service::AnalysisDependency {
                    from: "debug".to_string(),
                    to: "renderer".to_string(),
                    edge_type: crate::service::DesignEdgeType::Direct,
                },
                crate::service::AnalysisDependency {
                    from: "renderer".to_string(),
                    to: "debug".to_string(),
                    edge_type: crate::service::DesignEdgeType::Direct,
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
    fn multi_select_candidates_are_deterministic() {
        let root = sample_project();
        let analysis = sample_analysis(&root);
        let event = GuiAction {
            action: "refactor".to_string(),
            target: "auto".to_string(),
            node: None,
            project_root: Some(root),
            selected_nodes: vec!["debug".to_string(), "renderer".to_string()],
            selected_edges: vec![StructureEdge {
                from: "debug".to_string(),
                to: "renderer".to_string(),
            }],
            mode: GuiActionMode::Preview,
        };
        let lhs = build_refactor_candidates(&analysis, &event);
        let rhs = build_refactor_candidates(&analysis, &event);
        assert_eq!(lhs, rhs);
        assert!(
            lhs.iter()
                .any(|candidate| candidate.kind == RefactorActionKind::MergeModule)
        );
    }
}
