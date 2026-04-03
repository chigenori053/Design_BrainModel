use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::refactor::RefactorPlan;
use crate::service::{AnalysisReport, analyze_path};

use super::{
    ChangedEdge, DesignSyncStatus, HeatmapDelta, PreviewEdge, PreviewGraph, PreviewOverlay,
    StructureViewIR, ViewEdge, ViewNode, ViewerSelection, structure_ir_path,
};

pub fn export_structure_view(root: &Path) -> Result<StructureViewIR, String> {
    let analysis = analyze_path(root)?;
    let ir = build_structure_view_ir(&analysis, None);
    write_ir(root, &ir)?;
    Ok(ir)
}

pub fn export_structure_view_from_plan(
    root: &Path,
    plan: &RefactorPlan,
) -> Result<StructureViewIR, String> {
    let analysis = analyze_path(root)?;
    let ir = build_structure_view_ir(&analysis, Some(plan));
    write_ir(root, &ir)?;
    Ok(ir)
}

pub fn build_structure_view_ir(
    analysis: &AnalysisReport,
    plan: Option<&RefactorPlan>,
) -> StructureViewIR {
    let centrality = node_centrality(analysis);
    let layers = node_layers(analysis);
    let roles = node_roles(analysis);
    let mut nodes = analysis
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| {
            let layer = layers.get(&module.name).copied().unwrap_or(index % 3);
            let central = centrality.get(&module.name).copied().unwrap_or_default() as f32;
            ViewNode {
                id: module.name.clone(),
                label: module.name.clone(),
                layer,
                role: roles
                    .get(&module.name)
                    .cloned()
                    .unwrap_or_else(|| "module".to_string()),
                x: (index % 4) as f32 * 220.0 + layer as f32 * 18.0,
                y: central * 70.0 + 80.0,
                z: layer as f32 * 140.0,
            }
        })
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        let inferred = infer_modules_from_dependencies(analysis);
        nodes = inferred
            .into_iter()
            .enumerate()
            .map(|(index, name)| ViewNode {
                id: name.clone(),
                label: name.clone(),
                layer: index % 3,
                role: "module".to_string(),
                x: (index % 4) as f32 * 220.0,
                y: 120.0,
                z: (index % 3) as f32 * 140.0,
            })
            .collect();
    }

    let cycle_pairs = cycle_pairs(analysis);
    let edges = analysis
        .dependencies
        .iter()
        .map(|dependency| ViewEdge {
            from: dependency.from.clone(),
            to: dependency.to.clone(),
            kind: "depends_on".to_string(),
            cycle: cycle_pairs.contains(&(dependency.from.clone(), dependency.to.clone())),
        })
        .collect::<Vec<_>>();

    StructureViewIR {
        version: 2,
        nodes,
        edges,
        preview: plan.map(build_preview_overlay),
        snapshots: Vec::new(),
        history: Vec::new(),
        risk_overlay: Vec::new(),
        selection: ViewerSelection::default(),
        candidates: Vec::new(),
        heatmap: plan.map(build_heatmap).unwrap_or_default(),
        design_sync: DesignSyncStatus::default(),
    }
}

fn build_preview_overlay(plan: &RefactorPlan) -> PreviewOverlay {
    let before_graph = PreviewGraph {
        nodes: plan.before_graph.nodes.clone(),
        edges: plan
            .before_graph
            .edges
            .iter()
            .map(|edge| PreviewEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            })
            .collect(),
    };
    let after_graph = PreviewGraph {
        nodes: plan.after_graph.nodes.clone(),
        edges: plan
            .after_graph
            .edges
            .iter()
            .map(|edge| PreviewEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            })
            .collect(),
    };
    let mut changed_edges = Vec::new();
    for edge in &plan.removed_edges {
        changed_edges.push(ChangedEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            change: "removed".to_string(),
        });
    }
    for edge in &plan.after_graph.edges {
        let existed = plan
            .before_graph
            .edges
            .iter()
            .any(|before| before.from == edge.from && before.to == edge.to);
        if !existed {
            changed_edges.push(ChangedEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                change: "added".to_string(),
            });
        }
    }
    let moved_files = plan
        .moved_files
        .iter()
        .map(|(from, to)| format!("{} -> {}", from.display(), to.display()))
        .collect();
    PreviewOverlay {
        before_graph,
        after_graph,
        changed_edges,
        moved_files,
    }
}

fn build_heatmap(plan: &RefactorPlan) -> Vec<HeatmapDelta> {
    let mut heatmap = Vec::new();
    if !plan.removed_edges.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "green".to_string(),
            label: "reduced coupling".to_string(),
            magnitude: 0.9,
        });
    }
    if !plan.moved_files.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "blue".to_string(),
            label: "moved responsibility".to_string(),
            magnitude: 0.72,
        });
    }
    if heatmap.is_empty() {
        heatmap.push(HeatmapDelta {
            target: format!("{:?}", plan.target),
            color: "red".to_string(),
            label: "new risk under review".to_string(),
            magnitude: 0.4,
        });
    }
    heatmap
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
    .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn node_centrality(analysis: &AnalysisReport) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for dependency in &analysis.dependencies {
        *counts.entry(dependency.from.clone()).or_insert(0) += 1;
        *counts.entry(dependency.to.clone()).or_insert(0) += 1;
    }
    counts
}

fn node_layers(analysis: &AnalysisReport) -> BTreeMap<String, usize> {
    let mut layers = BTreeMap::new();
    for (level, layer) in analysis.layers.layers.iter().enumerate() {
        for node in &layer.nodes {
            layers.insert(node.clone(), level);
        }
    }
    layers
}

fn node_roles(analysis: &AnalysisReport) -> BTreeMap<String, String> {
    analysis
        .roles
        .iter()
        .map(|role| (role.node_name.clone(), format!("{:?}", role.score)))
        .collect()
}

fn cycle_pairs(analysis: &AnalysisReport) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for cycle in &analysis.cycles.cycles {
        for index in 0..cycle.nodes.len() {
            let from = cycle.nodes[index].clone();
            let to = cycle.nodes[(index + 1) % cycle.nodes.len()].clone();
            pairs.insert((from, to));
        }
    }
    pairs
}

fn infer_modules_from_dependencies(analysis: &AnalysisReport) -> BTreeSet<String> {
    let mut nodes = BTreeSet::new();
    for dependency in &analysis.dependencies {
        nodes.insert(dependency.from.clone());
        nodes.insert(dependency.to.clone());
    }
    nodes
}

#[cfg(test)]
mod tests {
    use crate::refactor::{RefactorTarget, create_refactor_plan};
    use crate::service::{
        AnalysisDependency, AnalysisModule, AnalysisReport, AnalysisSummary, DataFlowEdgeReport,
    };
    use integration_layer::{Cycle, CycleReport, Layer, LayerModel};

    use super::*;

    fn sample_analysis(root: &str) -> AnalysisReport {
        AnalysisReport {
            root: root.to_string(),
            total_files: 2,
            source_files: 2,
            avg_complexity: "1.0".to_string(),
            manifests: vec!["Cargo.toml".to_string()],
            languages: BTreeMap::new(),
            top_level_entries: vec!["src".to_string()],
            architecture_hints: vec!["has-tests".to_string()],
            modules: vec![
                AnalysisModule {
                    name: "renderer".to_string(),
                    file_count: 1,
                    source_path: "src/renderer.rs".to_string(),
                },
                AnalysisModule {
                    name: "debug".to_string(),
                    file_count: 1,
                    source_path: "src/debug.rs".to_string(),
                },
            ],
            graph_nodes: vec![
                crate::service::ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: "sample".to_string(),
                        module_path: "renderer".to_string(),
                    },
                    logical_name: "renderer".to_string(),
                    source_path: Some(std::path::PathBuf::from("src/renderer.rs")),
                },
                crate::service::ModuleNode {
                    qualified_id: crate::source_index::QualifiedModuleId {
                        crate_name: "sample".to_string(),
                        module_path: "debug".to_string(),
                    },
                    logical_name: "debug".to_string(),
                    source_path: Some(std::path::PathBuf::from("src/debug.rs")),
                },
            ],
            dependencies: vec![
                AnalysisDependency {
                    from: "renderer".to_string(),
                    to: "debug".to_string(),
                },
                AnalysisDependency {
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
            layers: LayerModel {
                layers: vec![Layer {
                    level: 0,
                    nodes: vec!["renderer".to_string(), "debug".to_string()],
                }],
            },
            violations: Vec::new(),
            roles: Vec::new(),
            semantic_layers: Vec::new(),
            data_flow: vec![DataFlowEdgeReport {
                from: "renderer".to_string(),
                to: "debug".to_string(),
                weight: 1.0,
            }],
            issues: Vec::new(),
            code_issues: Vec::new(),
            summary: AnalysisSummary::default(),
            next_action: String::new(),
            root_cause: None,
            refactor_plan: Vec::new(),
        }
    }

    #[test]
    fn exports_ir_with_preview_overlay() {
        let analysis = sample_analysis("/tmp/sample");
        let plan = create_refactor_plan(&analysis, RefactorTarget::Cycle).expect("plan");
        let ir = build_structure_view_ir(&analysis, Some(&plan));
        assert_eq!(ir.nodes.len(), 2);
        assert_eq!(ir.edges.len(), 2);
        assert!(ir.preview.is_some());
    }

    #[test]
    fn writes_ir_to_dbm_directory() {
        let unique = format!(
            "viewer_export_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(root.join("src")).expect("create root");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"viewer_export\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("lib");
        fs::write(root.join("src/renderer.rs"), "use crate::debug;\n").expect("renderer");
        fs::write(root.join("src/debug.rs"), "use crate::renderer;\n").expect("debug");
        let analysis = sample_analysis(root.to_str().unwrap());
        let plan = create_refactor_plan(&analysis, RefactorTarget::Cycle).expect("plan");
        let ir = build_structure_view_ir(&analysis, Some(&plan));
        write_ir(&root, &ir).expect("write");
        assert!(structure_ir_path(&root).exists());
    }
}
