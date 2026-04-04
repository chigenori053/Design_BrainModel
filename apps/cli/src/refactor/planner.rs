use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use integration_layer::{generate_patches, simulate_refactor};

use crate::service::AnalysisReport;

use super::{
    RefactorPlan, RefactorTarget, StructureEdge, StructureGraph, counts_by_node,
    graph_from_analysis, integration_plan_for_target, source_index_for_report,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchScope {
    WorkspaceWide,
    SameCrate,
    ExplicitTargetOnly,
}

pub fn default_target(report: &AnalysisReport) -> RefactorTarget {
    if report.cycles.has_cycle {
        RefactorTarget::Cycle
    } else if let Some(violation) = report.violations.first() {
        RefactorTarget::LayerViolation(format!("{}->{}", violation.from, violation.to))
    } else if let Some(module) = report.modules.first() {
        RefactorTarget::ModuleSplit(module.name.clone())
    } else {
        RefactorTarget::ModuleSplit("core".to_string())
    }
}

pub fn resolve_target(
    report: &AnalysisReport,
    target: Option<&str>,
    node: Option<&str>,
    file: Option<&std::path::Path>,
) -> RefactorTarget {
    match target.unwrap_or("cycle") {
        "cycle" => RefactorTarget::Cycle,
        "extract-interface" => {
            let edge = report
                .dependencies
                .first()
                .map(|edge| (edge.from.clone(), edge.to.clone()))
                .unwrap_or_else(|| ("module".to_string(), "dependency".to_string()));
            RefactorTarget::ExtractInterface {
                from: edge.0,
                to: edge.1,
            }
        }
        "remove-dependency" => {
            let edge = report
                .dependencies
                .first()
                .map(|edge| (edge.from.clone(), edge.to.clone()))
                .unwrap_or_else(|| ("module".to_string(), "dependency".to_string()));
            RefactorTarget::RemoveDependency {
                from: edge.0,
                to: edge.1,
            }
        }
        "module-split" => RefactorTarget::ModuleSplit(
            node.map(ToString::to_string)
                .or_else(|| report.modules.first().map(|module| module.name.clone()))
                .unwrap_or_else(|| "core".to_string()),
        ),
        "merge-module" => RefactorTarget::MergeModule(
            node.map(|value| {
                value
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .collect()
            })
            .unwrap_or_else(|| {
                report
                    .modules
                    .iter()
                    .take(2)
                    .map(|module| module.name.clone())
                    .collect()
            }),
        ),
        "layer-violation" => RefactorTarget::LayerViolation(
            node.map(ToString::to_string)
                .or_else(|| {
                    report
                        .violations
                        .first()
                        .map(|violation| format!("{}->{}", violation.from, violation.to))
                })
                .unwrap_or_else(|| "interface->domain".to_string()),
        ),
        "rename-boundary" => RefactorTarget::RenameBoundary(
            node.map(ToString::to_string)
                .or_else(|| report.modules.first().map(|module| module.name.clone()))
                .unwrap_or_else(|| "core".to_string()),
        ),
        "introduce-service" => RefactorTarget::IntroduceService(
            node.map(ToString::to_string)
                .or_else(|| report.modules.first().map(|module| module.name.clone()))
                .unwrap_or_else(|| "core".to_string()),
        ),
        "file-move" => RefactorTarget::FileMove(
            file.map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("src/lib.rs")),
        ),
        _ => default_target(report),
    }
}

pub fn create_refactor_plan(
    report: &AnalysisReport,
    target: RefactorTarget,
) -> Result<RefactorPlan, String> {
    let before_graph = graph_from_analysis(report);
    let (after_graph, removed_edges, moved_files, detail, confidence) = match &target {
        RefactorTarget::Cycle => plan_cycle_break(report, &before_graph)?,
        RefactorTarget::ExtractInterface { from, to } => {
            plan_extract_interface(&before_graph, from, to)
        }
        RefactorTarget::RemoveDependency { from, to } => {
            let mut after = before_graph.clone();
            after
                .edges
                .retain(|edge| !(edge.from == *from && edge.to == *to));
            (
                after,
                vec![StructureEdge {
                    from: from.clone(),
                    to: to.clone(),
                }],
                Vec::new(),
                Some(format!("{from}:{to}")),
                0.83,
            )
        }
        RefactorTarget::ModuleSplit(module) => {
            let mut after = before_graph.clone();
            for derived in [format!("{module}_core"), format!("{module}_api")] {
                if !after.nodes.iter().any(|node| node == &derived) {
                    after.nodes.push(derived);
                }
            }
            after.nodes.sort();
            (after, Vec::new(), Vec::new(), Some(module.clone()), 0.82)
        }
        RefactorTarget::LayerViolation(detail) => {
            let mut after = before_graph.clone();
            let parts = detail
                .split("->")
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>();
            if parts.len() == 2 {
                after
                    .edges
                    .retain(|edge| !(edge.from == parts[0] && edge.to == parts[1]));
                let interface = format!("{}_{}_interface", parts[0], parts[1]);
                if !after.nodes.iter().any(|node| node == &interface) {
                    after.nodes.push(interface.clone());
                }
                after.edges.push(StructureEdge {
                    from: parts[0].clone(),
                    to: interface.clone(),
                });
                after.edges.push(StructureEdge {
                    from: interface,
                    to: parts[1].clone(),
                });
                after.nodes.sort();
                after
                    .edges
                    .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
            }
            (after, Vec::new(), Vec::new(), Some(detail.clone()), 0.74)
        }
        RefactorTarget::MergeModule(modules) => {
            let merged = modules.join("_");
            let mut after = before_graph.clone();
            after.nodes.retain(|node| !modules.contains(node));
            after.nodes.push(merged.clone());
            for edge in &mut after.edges {
                if modules.contains(&edge.from) {
                    edge.from = merged.clone();
                }
                if modules.contains(&edge.to) {
                    edge.to = merged.clone();
                }
            }
            after.edges.retain(|edge| edge.from != edge.to);
            after.nodes.sort();
            after
                .edges
                .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
            after
                .edges
                .dedup_by(|lhs, rhs| lhs.from == rhs.from && lhs.to == rhs.to);
            (after, Vec::new(), Vec::new(), Some(merged), 0.68)
        }
        RefactorTarget::RenameBoundary(module) => {
            let renamed = format!("{module}_boundary");
            let mut after = before_graph.clone();
            for node in &mut after.nodes {
                if node == module {
                    *node = renamed.clone();
                }
            }
            for edge in &mut after.edges {
                if edge.from == *module {
                    edge.from = renamed.clone();
                }
                if edge.to == *module {
                    edge.to = renamed.clone();
                }
            }
            after.nodes.sort();
            after
                .edges
                .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
            (after, Vec::new(), Vec::new(), Some(module.clone()), 0.71)
        }
        RefactorTarget::IntroduceService(module) => {
            let service = format!("{module}_service");
            let mut after = before_graph.clone();
            if !after.nodes.iter().any(|node| node == &service) {
                after.nodes.push(service.clone());
            }
            after.edges.push(StructureEdge {
                from: module.clone(),
                to: service.clone(),
            });
            after.nodes.sort();
            after
                .edges
                .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
            (after, Vec::new(), Vec::new(), Some(module.clone()), 0.72)
        }
        RefactorTarget::FileMove(path) => {
            let after = before_graph.clone();
            let destination = PathBuf::from("src").join("moved").join(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("moved.rs"),
            );
            (
                after,
                Vec::new(),
                vec![(path.clone(), destination)],
                Some(
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("moved")
                        .to_string(),
                ),
                0.7,
            )
        }
    };

    let integration = integration_plan_for_target(&target, detail.as_deref());
    let simulation = simulate_refactor(&report_to_architecture_ir(report), &integration);
    let patches = generate_patches(&integration);
    let affected_files = affected_files_for_target(report, &target, &removed_edges, &moved_files);

    Ok(RefactorPlan {
        target,
        affected_files,
        before_graph,
        after_graph,
        confidence,
        root: PathBuf::from(&report.root),
        removed_edges,
        moved_files,
        estimated_delta: simulation.delta,
        patches,
    })
}

fn plan_extract_interface(
    before_graph: &StructureGraph,
    from: &str,
    to: &str,
) -> (
    StructureGraph,
    Vec<StructureEdge>,
    Vec<(PathBuf, PathBuf)>,
    Option<String>,
    f32,
) {
    let mut after = before_graph.clone();
    after
        .edges
        .retain(|edge| !(edge.from == from && edge.to == to));
    let interface = format!("{from}_{to}_interface");
    if !after.nodes.iter().any(|node| node == &interface) {
        after.nodes.push(interface.clone());
    }
    after.edges.push(StructureEdge {
        from: from.to_string(),
        to: interface.clone(),
    });
    after.edges.push(StructureEdge {
        from: interface,
        to: to.to_string(),
    });
    after.nodes.sort();
    after
        .edges
        .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
    (
        after,
        vec![StructureEdge {
            from: from.to_string(),
            to: to.to_string(),
        }],
        Vec::new(),
        Some(format!("{from}:{to}")),
        0.88,
    )
}

fn plan_cycle_break(
    report: &AnalysisReport,
    before_graph: &StructureGraph,
) -> Result<
    (
        StructureGraph,
        Vec<StructureEdge>,
        Vec<(PathBuf, PathBuf)>,
        Option<String>,
        f32,
    ),
    String,
> {
    let selected = report
        .cycles
        .cycles
        .first()
        .and_then(|cycle| {
            if cycle.nodes.len() < 2 {
                None
            } else {
                let centrality = counts_by_node(&report.dependencies);
                cycle_edges(&cycle.nodes)
                    .iter()
                    .min_by_key(|edge| edge_score(edge, &centrality))
                    .cloned()
            }
        })
        .or_else(|| fallback_cycle_edge(before_graph))
        .ok_or_else(|| "cycle target requested but no cycle found".to_string())?;

    let mut after = before_graph.clone();
    after
        .edges
        .retain(|edge| !(edge.from == selected.from && edge.to == selected.to));
    let interface = format!("{}_{}_interface", selected.from, selected.to);
    if !after.nodes.iter().any(|node| node == &interface) {
        after.nodes.push(interface.clone());
    }
    after.edges.push(StructureEdge {
        from: selected.from.clone(),
        to: interface.clone(),
    });
    after.edges.push(StructureEdge {
        from: interface,
        to: selected.to.clone(),
    });
    after.nodes.sort();
    after
        .edges
        .sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));

    Ok((
        after,
        vec![selected.clone()],
        Vec::new(),
        Some(format!("{}:{}", selected.from, selected.to)),
        0.9,
    ))
}

fn cycle_edges(nodes: &[String]) -> Vec<StructureEdge> {
    let mut edges = Vec::new();
    for index in 0..nodes.len() {
        let from = nodes[index].clone();
        let to = nodes[(index + 1) % nodes.len()].clone();
        edges.push(StructureEdge { from, to });
    }
    edges
}

fn fallback_cycle_edge(graph: &StructureGraph) -> Option<StructureEdge> {
    let mut candidates = graph
        .edges
        .iter()
        .filter(|edge| {
            graph
                .edges
                .iter()
                .any(|candidate| candidate.from == edge.to && candidate.to == edge.from)
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| (&lhs.from, &lhs.to).cmp(&(&rhs.from, &rhs.to)));
    candidates.into_iter().next()
}

fn edge_score(
    edge: &StructureEdge,
    centrality: &BTreeMap<String, usize>,
) -> (usize, String, String) {
    (
        centrality.get(&edge.from).copied().unwrap_or_default()
            + centrality.get(&edge.to).copied().unwrap_or_default(),
        edge.from.clone(),
        edge.to.clone(),
    )
}

fn report_to_architecture_ir(report: &AnalysisReport) -> integration_layer::ArchitectureIr {
    let source_index = source_index_for_report(report);
    let mut nodes = report
        .modules
        .iter()
        .map(|module| integration_layer::ArchitectureIrNode {
            id: module.name.clone(),
            name: module.name.clone(),
            kind: integration_layer::ArchitectureNodeKind::Module,
            file_path: source_index
                .resolve(&module.name)
                .ok()
                .flatten()
                .map(|path| path.display().to_string())
                .or_else(|| (!module.source_path.is_empty()).then(|| module.source_path.clone())),
        })
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        let graph = graph_from_analysis(report);
        nodes = graph
            .nodes
            .iter()
            .map(|node| integration_layer::ArchitectureIrNode {
                id: node.clone(),
                name: node.clone(),
                kind: integration_layer::ArchitectureNodeKind::Module,
                file_path: source_index
                    .resolve(node)
                    .ok()
                    .flatten()
                    .map(|path| path.display().to_string()),
            })
            .collect();
    }
    let edges = report
        .dependencies
        .iter()
        .map(|dependency| integration_layer::ArchitectureIrEdge {
            from: dependency.from.clone(),
            to: dependency.to.clone(),
            kind: integration_layer::ArchitectureEdgeKind::DependsOn,
        })
        .collect();
    integration_layer::ArchitectureIr {
        nodes,
        edges,
        metadata: integration_layer::ArchitectureIrMetadata {
            graph_id: report.root.clone(),
        },
    }
}

fn affected_files_for_target(
    report: &AnalysisReport,
    target: &RefactorTarget,
    removed_edges: &[StructureEdge],
    moved_files: &[(PathBuf, PathBuf)],
) -> Vec<PathBuf> {
    let source_index = source_index_for_report(report);
    let mut files = Vec::new();
    match target {
        RefactorTarget::Cycle => {
            for edge in removed_edges {
                if let Ok(Some(path)) = source_index.resolve(&edge.from) {
                    files.push(path);
                }
                if let Ok(Some(path)) = source_index.resolve(&edge.to) {
                    files.push(path);
                }
                files.push(source_index.generated_path(
                    PathBuf::from(&report.root).as_path(),
                    &edge.from,
                    &format!(
                        "{}_{}_interface.rs",
                        edge.from.replace('-', "_"),
                        edge.to.replace('-', "_")
                    ),
                ));
            }
        }
        RefactorTarget::ExtractInterface { from, to } => {
            if let Ok(Some(path)) = source_index.resolve(from) {
                files.push(path);
            }
            if let Ok(Some(path)) = source_index.resolve(to) {
                files.push(path);
            }
            files.push(source_index.generated_path(
                PathBuf::from(&report.root).as_path(),
                from,
                &format!(
                    "{}_{}_interface.rs",
                    from.replace('-', "_"),
                    to.replace('-', "_")
                ),
            ));
        }
        RefactorTarget::RemoveDependency { from, to } => {
            if let Ok(Some(path)) = source_index.resolve(from) {
                files.push(path);
            }
            if let Ok(Some(path)) = source_index.resolve(to) {
                files.push(path);
            }
        }
        RefactorTarget::ModuleSplit(module) => {
            if let Ok(Some(path)) = source_index.resolve(module) {
                files.push(path);
            }
            files.push(source_index.generated_path(
                PathBuf::from(&report.root).as_path(),
                module,
                &format!("{}_core.rs", module.replace('-', "_")),
            ));
            files.push(source_index.generated_path(
                PathBuf::from(&report.root).as_path(),
                module,
                &format!("{}_api.rs", module.replace('-', "_")),
            ));
        }
        RefactorTarget::MergeModule(modules) => {
            for module in modules {
                if let Ok(Some(path)) = source_index.resolve(module) {
                    files.push(path);
                }
            }
        }
        RefactorTarget::LayerViolation(detail) => {
            let parts = detail
                .split("->")
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>();
            if parts.len() == 2 {
                if let Ok(Some(path)) = source_index.resolve(&parts[0]) {
                    files.push(path);
                }
                files.push(source_index.generated_path(
                    PathBuf::from(&report.root).as_path(),
                    &parts[0],
                    &format!(
                        "{}_{}_interface.rs",
                        parts[0].replace('-', "_"),
                        parts[1].replace('-', "_")
                    ),
                ));
            }
        }
        RefactorTarget::RenameBoundary(module) | RefactorTarget::IntroduceService(module) => {
            if let Ok(Some(path)) = source_index.resolve(module) {
                files.push(path);
            }
        }
        RefactorTarget::FileMove(path) => {
            files.push(path.clone());
            if let Some((_, destination)) = moved_files.first() {
                files.push(destination.clone());
            }
        }
    }
    if files.is_empty() {
        for path in source_index.all_paths() {
            files.push(path.clone());
        }
    }
    files.sort();
    files.dedup();
    files
}
