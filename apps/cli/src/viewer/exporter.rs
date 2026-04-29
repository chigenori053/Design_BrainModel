use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::commands::analyze::project::{AnalyzeMode, AnalyzeOptions, analyze_with_options};
use crate::refactor::{RefactorPlan, candidate_for_target, generate_mock_preview_diff};
use crate::report::Language;
use crate::service::{AnalysisReport, analyze_path};
use crate::source_index::ModuleSourceIndex;

use super::{
    CameraMode, CameraPreset3D, CandidateMove3D, Cluster3D, DesignSyncStatus, Edge3D, EdgeDelta,
    GraphDeltaAnimation, GraphSnapshot3D, HeatmapDelta, LayerPlane3D, Node3D, NodeMoveDelta,
    RefactorOverlay3D, RuntimePath3D, RuntimePathKind, SemanticGraph3D, Structure3DIr,
    StructureViewIR, TelemetryOverlay3D, Timeline3D, Vec3, ViewEdge, ViewNode, ViewerOverlays3D,
    ViewerSelection, structure_ir_path, sync_apply_preview_with_selection,
    sync_preview_with_selection, sync_transaction_execution_with_selection,
    sync_transaction_preview_with_selection,
};

pub fn export_structure_view(root: &Path) -> Result<StructureViewIR, String> {
    let analysis = analyze_path(root)?;
    let mut ir = build_structure_view_ir(&analysis, None);
    if let Some(root_str) = root.to_str() {
        let options = AnalyzeOptions {
            path: root_str.to_string(),
            mode: AnalyzeMode::Summary,
            report: false,
            design: false,
            language: Language::English,
            intent: None,
            json: false,
            design_json: false,
        };
        if let Ok(unified) = analyze_with_options(&options) {
            super::inject_recommendation_candidates(&mut ir, &unified);
        }
    }
    sync_preview_with_selection(&mut ir);
    sync_apply_preview_with_selection(&mut ir);
    sync_transaction_preview_with_selection(&mut ir);
    sync_transaction_execution_with_selection(&mut ir);
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
    let runtime_depths = runtime_depths(analysis);
    let source_bindings = source_bindings(analysis);
    let scene_3d = build_scene_3d(
        analysis,
        plan,
        &centrality,
        &layers,
        &roles,
        &runtime_depths,
        &source_bindings,
    );
    let scene_nodes = scene_3d
        .graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = analysis
        .modules
        .iter()
        .map(|module| {
            let layer = layers.get(&module.name).copied().unwrap_or_default();
            let scene = scene_nodes.get(&module.name).copied();
            ViewNode {
                id: module.name.clone(),
                label: module.name.clone(),
                layer,
                role: roles
                    .get(&module.name)
                    .cloned()
                    .unwrap_or_else(|| "module".to_string()),
                x: scene.map(|node| node.position.x).unwrap_or_default(),
                y: scene.map(|node| node.position.y).unwrap_or_default(),
                z: scene.map(|node| node.position.z).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        let inferred = infer_modules_from_dependencies(analysis);
        nodes = inferred
            .into_iter()
            .map(|name| {
                let scene = scene_nodes.get(&name).copied();
                ViewNode {
                    id: name.clone(),
                    label: name.clone(),
                    layer: layers.get(&name).copied().unwrap_or_default(),
                    role: roles
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(|| "module".to_string()),
                    x: scene.map(|node| node.position.x).unwrap_or_default(),
                    y: scene.map(|node| node.position.y).unwrap_or_default(),
                    z: scene.map(|node| node.position.z).unwrap_or_default(),
                }
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
        preview: plan
            .map(|plan| generate_mock_preview_diff(&candidate_for_target(analysis, &plan.target))),
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
        heatmap: plan.map(build_heatmap).unwrap_or_default(),
        design_sync: DesignSyncStatus::default(),
        scene_3d: Some(scene_3d),
    }
}

fn build_scene_3d(
    analysis: &AnalysisReport,
    plan: Option<&RefactorPlan>,
    centrality: &BTreeMap<String, usize>,
    layers: &BTreeMap<String, usize>,
    roles: &BTreeMap<String, String>,
    runtime_depths: &BTreeMap<String, usize>,
    source_bindings: &BTreeMap<String, super::SourceBinding>,
) -> Structure3DIr {
    let graph = build_semantic_graph_3d(
        analysis,
        centrality,
        layers,
        roles,
        runtime_depths,
        source_bindings,
    );
    let runtime_paths = build_runtime_paths(analysis, plan, &graph);
    let overlays = build_overlays_3d(analysis, plan, &graph, &runtime_paths);
    let timeline = build_timeline_3d(plan, &graph);
    let camera = CameraPreset3D {
        focus_cluster: overlays
            .refactor
            .as_ref()
            .and_then(|overlay| overlay.selected_nodes.first().cloned()),
        mode: if overlays.refactor.is_some() {
            CameraMode::RefactorPreview
        } else if !runtime_paths.is_empty() {
            CameraMode::RuntimeFlow
        } else {
            CameraMode::Architectural
        },
    };
    Structure3DIr {
        graph,
        runtime_paths,
        overlays,
        timeline,
        camera,
    }
}

fn build_semantic_graph_3d(
    analysis: &AnalysisReport,
    centrality: &BTreeMap<String, usize>,
    layers: &BTreeMap<String, usize>,
    roles: &BTreeMap<String, String>,
    runtime_depths: &BTreeMap<String, usize>,
    source_bindings: &BTreeMap<String, super::SourceBinding>,
) -> SemanticGraph3D {
    let node_names = infer_modules_from_dependencies(analysis);
    let max_centrality = centrality.values().copied().max().unwrap_or(1) as f32;
    let nodes = node_names
        .iter()
        .map(|name| {
            let role = roles
                .get(name)
                .cloned()
                .unwrap_or_else(|| infer_role(name, layers.get(name).copied().unwrap_or_default()));
            let importance_raw = centrality.get(name).copied().unwrap_or_default() as f32;
            let importance = (importance_raw / max_centrality).clamp(0.05, 1.0);
            let heat = node_heat(name, analysis, importance);
            let semantic_lane = layers.get(name).copied().unwrap_or_default();
            let position = Vec3 {
                x: semantic_axis_x(semantic_lane),
                y: deterministic_importance_y(name, semantic_lane, &role, importance),
                z: deterministic_runtime_z(
                    name,
                    semantic_lane,
                    &role,
                    runtime_depths.get(name).copied().unwrap_or_default(),
                ),
            };
            Node3D {
                id: name.clone(),
                label: name.clone(),
                kind: role,
                position,
                size: 14.0 + importance * 16.0,
                importance,
                heat,
                source_binding: source_bindings.get(name).cloned(),
            }
        })
        .collect::<Vec<_>>();
    let edges = analysis
        .dependencies
        .iter()
        .map(|dependency| Edge3D {
            from: dependency.from.clone(),
            to: dependency.to.clone(),
            weight: edge_weight(analysis, &dependency.from, &dependency.to),
            edge_kind: "depends_on".to_string(),
            violation: analysis.violations.iter().any(|violation| {
                violation.from == dependency.from && violation.to == dependency.to
            }),
        })
        .collect::<Vec<_>>();
    let layers_3d = layer_planes();
    let clusters = build_clusters(&nodes);
    SemanticGraph3D {
        nodes,
        edges,
        clusters,
        layers: layers_3d,
    }
}

fn build_runtime_paths(
    analysis: &AnalysisReport,
    plan: Option<&RefactorPlan>,
    graph: &SemanticGraph3D,
) -> Vec<RuntimePath3D> {
    let positions = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect::<BTreeMap<_, _>>();
    let mut paths = Vec::new();
    let execution_points = analysis
        .data_flow
        .iter()
        .filter_map(|edge| positions.get(&edge.from).copied())
        .chain(
            analysis
                .data_flow
                .last()
                .and_then(|edge| positions.get(&edge.to).copied()),
        )
        .collect::<Vec<_>>();
    if execution_points.len() >= 2 {
        paths.push(RuntimePath3D {
            id: "execution".to_string(),
            points: execution_points,
            path_kind: RuntimePathKind::Execution,
            animated: true,
        });
    }
    let validation_points = graph
        .edges
        .iter()
        .filter(|edge| edge.violation)
        .flat_map(|edge| {
            [
                positions.get(&edge.from).copied(),
                positions.get(&edge.to).copied(),
            ]
        })
        .flatten()
        .collect::<Vec<_>>();
    if validation_points.len() >= 2 {
        paths.push(RuntimePath3D {
            id: "validation".to_string(),
            points: validation_points,
            path_kind: RuntimePathKind::Validation,
            animated: true,
        });
    }
    let rollback_points = graph
        .edges
        .iter()
        .filter(|edge| edge.violation)
        .rev()
        .flat_map(|edge| {
            [
                positions.get(&edge.to).copied(),
                positions.get(&edge.from).copied(),
            ]
        })
        .flatten()
        .collect::<Vec<_>>();
    if rollback_points.len() >= 2 {
        paths.push(RuntimePath3D {
            id: "rollback".to_string(),
            points: rollback_points,
            path_kind: RuntimePathKind::Rollback,
            animated: true,
        });
    }
    if let Some(plan) = plan {
        let preview_points = plan
            .removed_edges
            .iter()
            .flat_map(|edge| {
                [
                    positions.get(&edge.from).copied(),
                    positions.get(&edge.to).copied(),
                ]
            })
            .flatten()
            .collect::<Vec<_>>();
        if preview_points.len() >= 2 {
            paths.push(RuntimePath3D {
                id: "refactor-preview".to_string(),
                points: preview_points,
                path_kind: RuntimePathKind::RefactorPreview,
                animated: true,
            });
        }
    }
    let memory_release_points = analysis
        .dependencies
        .iter()
        .rev()
        .take(3)
        .flat_map(|edge| {
            [
                positions.get(&edge.to).copied(),
                positions.get(&edge.from).copied(),
            ]
        })
        .flatten()
        .collect::<Vec<_>>();
    if memory_release_points.len() >= 2 {
        paths.push(RuntimePath3D {
            id: "memory-release".to_string(),
            points: memory_release_points,
            path_kind: RuntimePathKind::MemoryRelease,
            animated: true,
        });
    }
    paths
}

fn build_overlays_3d(
    analysis: &AnalysisReport,
    plan: Option<&RefactorPlan>,
    graph: &SemanticGraph3D,
    runtime_paths: &[RuntimePath3D],
) -> ViewerOverlays3D {
    let positions = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect::<BTreeMap<_, _>>();
    let refactor = plan.map(|plan| RefactorOverlay3D {
        selected_nodes: plan.after_graph.nodes.iter().take(4).cloned().collect(),
        candidate_moves: plan
            .moved_files
            .iter()
            .filter_map(|(from, to)| {
                let from_name = from.file_stem()?.to_str()?.to_string();
                let to_name = to.file_stem()?.to_str()?.to_string();
                let start = positions.get(&from_name).copied().unwrap_or_default();
                let mut end = positions.get(&to_name).copied().unwrap_or(start);
                end.x += 6.0;
                Some(CandidateMove3D {
                    node_id: from_name,
                    from: start,
                    to: end,
                    reason: format!("candidate move -> {}", to.display()),
                })
            })
            .collect(),
        predicted_cycle_reduction: analysis
            .cycles
            .cycles
            .len()
            .saturating_sub(plan.removed_edges.len()),
    });
    let telemetry = Some(TelemetryOverlay3D {
        hot_path_count: runtime_paths
            .iter()
            .filter(|path| matches!(path.path_kind, RuntimePathKind::Execution))
            .count(),
        rollback_count: runtime_paths
            .iter()
            .filter(|path| matches!(path.path_kind, RuntimePathKind::Rollback))
            .count(),
        memory_release_count: runtime_paths
            .iter()
            .filter(|path| matches!(path.path_kind, RuntimePathKind::MemoryRelease))
            .count(),
    });
    ViewerOverlays3D {
        refactor,
        telemetry,
        source_jump: graph.nodes.iter().any(|node| node.source_binding.is_some()),
        design_sync: true,
    }
}

fn build_timeline_3d(plan: Option<&RefactorPlan>, graph: &SemanticGraph3D) -> Timeline3D {
    let mut snapshots = vec![GraphSnapshot3D {
        label: "before".to_string(),
        tick: 0,
        animation: GraphDeltaAnimation::default(),
    }];
    if let Some(plan) = plan {
        let preview_animation = build_delta_animation(graph, plan);
        snapshots.push(GraphSnapshot3D {
            label: "preview".to_string(),
            tick: 1,
            animation: preview_animation.clone(),
        });
        snapshots.push(GraphSnapshot3D {
            label: "apply".to_string(),
            tick: 2,
            animation: preview_animation.clone(),
        });
        snapshots.push(GraphSnapshot3D {
            label: "rollback".to_string(),
            tick: 3,
            animation: reverse_delta_animation(&preview_animation),
        });
    }
    Timeline3D {
        snapshots,
        current_tick: 0,
        autoplay: plan.is_some(),
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
        .map(|role| (role.node_name.clone(), format!("{:?}", role.role)))
        .collect()
}

fn source_bindings(analysis: &AnalysisReport) -> BTreeMap<String, super::SourceBinding> {
    let root = Path::new(&analysis.root);
    let index = ModuleSourceIndex::build(root).unwrap_or_default();
    analysis
        .graph_nodes
        .iter()
        .filter_map(|node| {
            index
                .exact_binding(root, &node.logical_name)
                .map(|binding| super::SourceBinding {
                    file: binding.file,
                    line_start: binding.line_start,
                    line_end: binding.line_end,
                    symbol: binding.symbol,
                })
                .or_else(|| {
                    node.source_path.as_ref().map(|path| super::SourceBinding {
                        file: path.clone(),
                        line_start: 1,
                        line_end: 1,
                        symbol: Some(node.logical_name.clone()),
                    })
                })
                .map(|binding| (node.logical_name.clone(), binding))
        })
        .collect()
}

fn runtime_depths(analysis: &AnalysisReport) -> BTreeMap<String, usize> {
    let mut depth = BTreeMap::new();
    let nodes = infer_modules_from_dependencies(analysis);
    for dependency in &analysis.dependencies {
        depth.entry(dependency.from.clone()).or_insert(0);
        depth.entry(dependency.to.clone()).or_insert(0);
    }
    for _ in 0..nodes.len().max(1) {
        let mut updated = false;
        for dependency in &analysis.dependencies {
            let next_depth = depth.get(&dependency.from).copied().unwrap_or_default() + 1;
            let entry = depth.entry(dependency.to.clone()).or_insert(0);
            if next_depth > *entry {
                *entry = next_depth.min(nodes.len());
                updated = true;
            }
        }
        if !updated {
            break;
        }
    }
    depth
}

fn semantic_axis_x(layer: usize) -> f32 {
    match layer {
        0 => 0.0,
        1 => 10.0,
        2 => 20.0,
        3 => 30.0,
        _ => 10.0 * layer as f32,
    }
}

fn infer_role(name: &str, layer: usize) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.contains("core") {
        "Core".to_string()
    } else if lower.contains("ui") || lower.contains("api") || lower.contains("interface") {
        "Interface".to_string()
    } else if lower.contains("db") || lower.contains("infra") || lower.contains("store") {
        "Infrastructure".to_string()
    } else if layer == 0 {
        "Core".to_string()
    } else if layer <= 1 {
        "Application".to_string()
    } else {
        "Interface".to_string()
    }
}

fn edge_weight(analysis: &AnalysisReport, from: &str, to: &str) -> f32 {
    analysis
        .data_flow
        .iter()
        .find(|edge| edge.from == from && edge.to == to)
        .map(|edge| edge.weight)
        .unwrap_or(1.0)
}

fn node_heat(name: &str, analysis: &AnalysisReport, importance: f32) -> f32 {
    let data_flow_boost = analysis
        .data_flow
        .iter()
        .filter(|edge| edge.from == name || edge.to == name)
        .map(|edge| edge.weight)
        .sum::<f32>();
    (importance + data_flow_boost * 0.25).clamp(0.0, 1.0)
}

fn layer_planes() -> Vec<LayerPlane3D> {
    vec![
        LayerPlane3D {
            level: 0,
            label: "Core".to_string(),
            axis_x: 0.0,
            color: "blue".to_string(),
        },
        LayerPlane3D {
            level: 1,
            label: "Application".to_string(),
            axis_x: 10.0,
            color: "yellow".to_string(),
        },
        LayerPlane3D {
            level: 2,
            label: "Interface".to_string(),
            axis_x: 20.0,
            color: "white".to_string(),
        },
        LayerPlane3D {
            level: 3,
            label: "Infrastructure".to_string(),
            axis_x: 30.0,
            color: "green".to_string(),
        },
    ]
}

fn build_clusters(nodes: &[Node3D]) -> Vec<Cluster3D> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for node in nodes {
        groups
            .entry(node.kind.clone())
            .or_default()
            .push(node.id.clone());
    }
    groups
        .into_iter()
        .map(|(label, nodes)| Cluster3D {
            id: label.to_ascii_lowercase(),
            label: label.clone(),
            nodes,
            color: cluster_color(&label).to_string(),
        })
        .collect()
}

fn cluster_color(label: &str) -> &'static str {
    let lower = label.to_ascii_lowercase();
    if lower.contains("core") {
        "blue"
    } else if lower.contains("infra") {
        "red"
    } else if lower.contains("interface") {
        "white"
    } else {
        "green"
    }
}

fn stable_spread(seed_parts: &[&str], scale: f32) -> f32 {
    let mut hash = 1469598103934665603_u64;
    for part in seed_parts {
        for byte in part.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        hash ^= 255;
        hash = hash.wrapping_mul(1099511628211);
    }
    let unit = (hash % 10_000) as f32 / 10_000.0;
    (unit - 0.5) * scale
}

fn deterministic_importance_y(
    name: &str,
    semantic_layer: usize,
    runtime_role: &str,
    importance: f32,
) -> f32 {
    120.0
        + importance * 520.0
        + stable_spread(
            &[name, &semantic_layer.to_string(), runtime_role, "y"],
            14.0,
        )
}

fn deterministic_runtime_z(
    name: &str,
    semantic_layer: usize,
    runtime_role: &str,
    runtime_depth: usize,
) -> f32 {
    runtime_depth as f32 * 110.0
        + stable_spread(
            &[name, &semantic_layer.to_string(), runtime_role, "z"],
            18.0,
        )
}

fn build_delta_animation(graph: &SemanticGraph3D, plan: &RefactorPlan) -> GraphDeltaAnimation {
    let positions = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect::<BTreeMap<_, _>>();
    GraphDeltaAnimation {
        moved_nodes: plan
            .moved_files
            .iter()
            .filter_map(|(from, to)| {
                let node_id = from.file_stem()?.to_str()?.to_string();
                let before = positions.get(&node_id).copied()?;
                let mut after = before;
                after.x += 10.0;
                after.z += 24.0;
                after.y += 18.0;
                if let Some(target_name) = to.file_stem().and_then(|stem| stem.to_str())
                    && let Some(target) = positions.get(target_name).copied()
                {
                    after.x = target.x;
                }
                Some(NodeMoveDelta {
                    node_id,
                    before,
                    after,
                })
            })
            .collect(),
        added_edges: plan
            .after_graph
            .edges
            .iter()
            .filter(|edge| {
                !plan
                    .before_graph
                    .edges
                    .iter()
                    .any(|before| before.from == edge.from && before.to == edge.to)
            })
            .map(|edge| EdgeDelta {
                from: edge.from.clone(),
                to: edge.to.clone(),
                kind: "depends_on".to_string(),
                violation_before: false,
                violation_after: false,
            })
            .collect(),
        removed_edges: plan
            .removed_edges
            .iter()
            .map(|edge| EdgeDelta {
                from: edge.from.clone(),
                to: edge.to.clone(),
                kind: "depends_on".to_string(),
                violation_before: true,
                violation_after: false,
            })
            .collect(),
        duration_ms: 900,
    }
}

fn reverse_delta_animation(animation: &GraphDeltaAnimation) -> GraphDeltaAnimation {
    GraphDeltaAnimation {
        moved_nodes: animation
            .moved_nodes
            .iter()
            .map(|delta| NodeMoveDelta {
                node_id: delta.node_id.clone(),
                before: delta.after,
                after: delta.before,
            })
            .collect(),
        added_edges: animation.removed_edges.clone(),
        removed_edges: animation.added_edges.clone(),
        duration_ms: animation.duration_ms,
    }
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
                    from: "debug".to_string(),
                    to: "renderer".to_string(),
                    edge_type: crate::service::DesignEdgeType::Direct,
                },
                AnalysisDependency {
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

    #[test]
    fn stable_3d_coordinates() {
        let analysis = sample_analysis("/tmp/sample");
        let left = build_structure_view_ir(&analysis, None);
        let right = build_structure_view_ir(&analysis, None);
        let left_nodes = left
            .scene_3d
            .as_ref()
            .expect("left scene")
            .graph
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.position))
            .collect::<BTreeMap<_, _>>();
        let right_nodes = right
            .scene_3d
            .as_ref()
            .expect("right scene")
            .graph
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.position))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(left_nodes, right_nodes);
    }
}
