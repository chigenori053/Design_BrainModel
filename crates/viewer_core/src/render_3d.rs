use std::collections::{BTreeMap, BTreeSet};

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};

use crate::animation::{animated_path, morph};
use crate::camera::frame_for_preset;
use crate::model::{
    CameraMode, Cluster3D, GraphDeltaAnimation, GraphSnapshot3D, LayerPlane3D, Node3D,
    RuntimePath3D, RuntimePathKind, SemanticGraph3D, Structure3DIr, StructureViewIR, Vec3,
};
use crate::projection_3d::ScreenProjector;
use crate::timeline::resolve_tick;

#[derive(Clone, Default)]
struct SceneState {
    show_runtime: bool,
    violation_only: bool,
    hot_path_only: bool,
    diff_preview: bool,
    hidden_layers: BTreeSet<usize>,
    forced_tick: Option<usize>,
    camera_mode: Option<CameraMode>,
}

pub fn render(ui: &mut egui::Ui, ir: &StructureViewIR, selected: &mut Option<String>) {
    let mut scene = ir.scene_3d.clone().unwrap_or_else(|| synthesize_scene(ir));
    if scene.graph.nodes.is_empty() {
        let desired = egui::vec2(ui.available_width(), ui.available_height().max(200.0));
        let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
        ui.painter_at(rect).text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No 3D scene available",
            FontId::proportional(14.0),
            Color32::from_gray(150),
        );
        return;
    }

    let state_id = egui::Id::new("viewer_core_render_3d_state");
    let mut state = ui
        .ctx()
        .data_mut(|data| data.get_temp::<SceneState>(state_id).unwrap_or_default());
    render_controls(ui, &mut scene, &mut state);

    let desired = egui::vec2(ui.available_width(), ui.available_height().max(240.0));
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, Color32::from_rgb(244, 240, 232));

    let time = ui.input(|input| input.time);
    if let Some(mode) = state.camera_mode {
        scene.camera.mode = mode;
    }
    let tick = state
        .forced_tick
        .unwrap_or_else(|| resolve_tick(&scene.timeline, time));
    let graph = graph_for_tick(&scene, tick);
    let camera = frame_for_preset(&scene.camera, time);
    let points = collect_points(&graph, &scene.runtime_paths, &state, time);
    let projector = ScreenProjector::new(&points, rect, camera);

    paint_layer_planes(&painter, rect, &graph.layers, &projector, &state);
    paint_edges(&painter, &graph, &projector, &state);
    if state.show_runtime {
        paint_runtime_paths(&painter, &scene.runtime_paths, &projector, &state, time);
    }
    paint_nodes(
        &painter,
        &graph,
        &projector,
        &state,
        selected,
        response.interact_pointer_pos(),
        time,
    );
    paint_hud(&painter, rect, &scene, tick);

    ui.ctx().data_mut(|data| data.insert_temp(state_id, state));
}

fn render_controls(ui: &mut egui::Ui, scene: &mut Structure3DIr, state: &mut SceneState) {
    let current_mode = state.camera_mode.unwrap_or(scene.camera.mode);
    ui.horizontal_wrapped(|ui| {
        ui.toggle_value(&mut state.show_runtime, "Runtime");
        ui.toggle_value(&mut state.violation_only, "Violation Only");
        ui.toggle_value(&mut state.hot_path_only, "Hot Path");
        ui.toggle_value(&mut state.diff_preview, "Diff Preview");

        if ui
            .selectable_label(
                matches!(current_mode, CameraMode::Architectural),
                "Architectural",
            )
            .clicked()
        {
            state.camera_mode = Some(CameraMode::Architectural);
        }
        if ui
            .selectable_label(matches!(current_mode, CameraMode::RuntimeFlow), "Runtime")
            .clicked()
        {
            state.camera_mode = Some(CameraMode::RuntimeFlow);
        }
        if ui
            .selectable_label(
                matches!(current_mode, CameraMode::RefactorPreview),
                "Preview",
            )
            .clicked()
        {
            state.camera_mode = Some(CameraMode::RefactorPreview);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for layer in &scene.graph.layers {
            let hidden = state.hidden_layers.contains(&layer.level);
            let label = if hidden {
                format!("Show {}", layer.label)
            } else {
                format!("Hide {}", layer.label)
            };
            if ui.small_button(label).clicked() {
                if hidden {
                    state.hidden_layers.remove(&layer.level);
                } else {
                    state.hidden_layers.insert(layer.level);
                }
            }
        }
    });
    if !scene.timeline.snapshots.is_empty() {
        let mut tick = state.forced_tick.unwrap_or(scene.timeline.current_tick);
        ui.horizontal(|ui| {
            ui.label("Timeline");
            ui.add(
                egui::Slider::new(&mut tick, 0..=scene.timeline.snapshots.len() - 1)
                    .show_value(true)
                    .text("tick"),
            );
            ui.checkbox(&mut scene.timeline.autoplay, "Autoplay");
            ui.label(scene.timeline.snapshots[tick].label.clone());
        });
        state.forced_tick = Some(tick);
    }
    ui.add_space(6.0);
}

fn paint_layer_planes(
    painter: &egui::Painter,
    rect: Rect,
    layers: &[LayerPlane3D],
    projector: &ScreenProjector,
    state: &SceneState,
) {
    for layer in layers {
        if state.hidden_layers.contains(&layer.level) {
            continue;
        }
        let x = layer.axis_x;
        let corners = [
            Vec3 {
                x,
                y: 80.0,
                z: -20.0,
            },
            Vec3 {
                x,
                y: 680.0,
                z: -20.0,
            },
            Vec3 {
                x,
                y: 680.0,
                z: 480.0,
            },
            Vec3 {
                x,
                y: 80.0,
                z: 480.0,
            },
        ];
        let pts = corners
            .iter()
            .map(|point| projector.project(*point))
            .collect::<Vec<_>>();
        painter.add(egui::Shape::convex_polygon(
            pts,
            layer_color(&layer.color, 18),
            Stroke::new(1.0, layer_color(&layer.color, 96)),
        ));
        let label_pos = projector.project(Vec3 {
            x,
            y: 700.0,
            z: 0.0,
        });
        painter.text(
            Pos2::new(
                label_pos.x.clamp(rect.left() + 20.0, rect.right() - 20.0),
                label_pos.y,
            ),
            Align2::CENTER_BOTTOM,
            &layer.label,
            FontId::proportional(11.0),
            Color32::from_gray(70),
        );
    }
}

fn paint_edges(
    painter: &egui::Painter,
    graph: &SemanticGraph3D,
    projector: &ScreenProjector,
    state: &SceneState,
) {
    let positions = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect::<BTreeMap<_, _>>();
    for edge in &graph.edges {
        if state.violation_only && !edge.violation {
            continue;
        }
        let Some(from) = positions.get(&edge.from).copied() else {
            continue;
        };
        let Some(to) = positions.get(&edge.to).copied() else {
            continue;
        };
        let a = projector.project(from);
        let b = projector.project(to);
        let stroke = if edge.violation {
            Stroke::new(2.0, Color32::from_rgb(196, 73, 61))
        } else {
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 100, 120, 128))
        };
        painter.line_segment([a, b], stroke);
    }
}

fn paint_runtime_paths(
    painter: &egui::Painter,
    paths: &[RuntimePath3D],
    projector: &ScreenProjector,
    state: &SceneState,
    time: f64,
) {
    for path in paths {
        if state.hot_path_only && !matches!(path.path_kind, RuntimePathKind::Execution) {
            continue;
        }
        let points = animated_path(path, time);
        for segment in points.windows(2) {
            let a = projector.project(segment[0]);
            let b = projector.project(segment[1]);
            painter.line_segment([a, b], Stroke::new(2.5, path_color(&path.path_kind)));
        }
    }
}

fn paint_nodes(
    painter: &egui::Painter,
    graph: &SemanticGraph3D,
    projector: &ScreenProjector,
    state: &SceneState,
    selected: &mut Option<String>,
    pointer_pos: Option<Pos2>,
    time: f64,
) {
    let mut nodes = graph.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        left.position
            .z
            .partial_cmp(&right.position.z)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for node in nodes {
        let layer = layer_for_x(node.position.x);
        if state.hidden_layers.contains(&layer) {
            continue;
        }
        if state.hot_path_only && node.heat < 0.55 {
            continue;
        }
        let mut position = node.position;
        if state.diff_preview {
            position = morph(
                node.position,
                Vec3 {
                    x: node.position.x + if node.heat > 0.6 { 2.0 } else { 0.0 },
                    y: node.position.y + node.importance * 14.0,
                    z: node.position.z + if node.heat > 0.6 { 10.0 } else { 0.0 },
                },
                time,
            );
        }
        let pos = projector.project(position);
        let is_selected = selected.as_deref() == Some(node.id.as_str());
        let radius = (node.size * 0.6).clamp(9.0, 22.0);
        let fill = if is_selected {
            Color32::from_rgb(28, 113, 184)
        } else {
            node_color(node)
        };
        painter.circle_filled(
            pos + egui::vec2(2.0, 3.0),
            radius,
            Color32::from_rgba_unmultiplied(0, 0, 0, 28),
        );
        painter.circle_filled(pos, radius, fill);
        if is_selected {
            painter.circle_stroke(pos, radius + 3.0, Stroke::new(1.8, Color32::WHITE));
        }
        painter.text(
            pos + egui::vec2(0.0, radius + 6.0),
            Align2::CENTER_TOP,
            format!("{}\n{}", node.label, node.kind),
            FontId::proportional(11.0),
            Color32::from_gray(22),
        );
        if let Some(pointer) = pointer_pos {
            if pointer.distance(pos) <= radius + 4.0 {
                *selected = Some(node.id.clone());
            }
        }
    }
}

fn paint_hud(painter: &egui::Painter, rect: Rect, scene: &Structure3DIr, tick: usize) {
    let timeline_label = scene
        .timeline
        .snapshots
        .get(tick)
        .map(|snapshot| snapshot.label.as_str())
        .unwrap_or("before");
    let telemetry = scene.overlays.telemetry.as_ref();
    let text = format!(
        "tick: {}  phase: {}  runtime: {}  hot: {}  rollback: {}",
        tick,
        timeline_label,
        scene.runtime_paths.len(),
        telemetry.map(|item| item.hot_path_count).unwrap_or(0),
        telemetry.map(|item| item.rollback_count).unwrap_or(0),
    );
    painter.text(
        Pos2::new(rect.left() + 16.0, rect.top() + 14.0),
        Align2::LEFT_TOP,
        text,
        FontId::proportional(11.0),
        Color32::from_gray(60),
    );
}

fn graph_for_tick(scene: &Structure3DIr, tick: usize) -> SemanticGraph3D {
    let snapshot = scene.timeline.snapshots.get(tick);
    match snapshot {
        Some(snapshot) if tick > 0 => apply_graph_animation(&scene.graph, &snapshot.animation),
        _ => scene.graph.clone(),
    }
}

fn collect_points(
    graph: &SemanticGraph3D,
    paths: &[RuntimePath3D],
    state: &SceneState,
    time: f64,
) -> Vec<Vec3> {
    let mut points = graph
        .nodes
        .iter()
        .map(|node| node.position)
        .collect::<Vec<_>>();
    for layer in &graph.layers {
        if state.hidden_layers.contains(&layer.level) {
            continue;
        }
        points.push(Vec3 {
            x: layer.axis_x,
            y: 700.0,
            z: 0.0,
        });
    }
    if state.show_runtime {
        for path in paths {
            points.extend(animated_path(path, time));
        }
    }
    points
}

fn layer_color(color: &str, alpha: u8) -> Color32 {
    match color {
        "blue" => Color32::from_rgba_unmultiplied(90, 135, 194, alpha),
        "red" => Color32::from_rgba_unmultiplied(194, 92, 82, alpha),
        "yellow" => Color32::from_rgba_unmultiplied(218, 190, 86, alpha),
        "green" => Color32::from_rgba_unmultiplied(95, 164, 112, alpha),
        _ => Color32::from_rgba_unmultiplied(180, 180, 180, alpha),
    }
}

fn path_color(kind: &RuntimePathKind) -> Color32 {
    match kind {
        RuntimePathKind::Execution => Color32::WHITE,
        RuntimePathKind::Validation => Color32::from_rgb(230, 192, 72),
        RuntimePathKind::Rollback => Color32::from_rgb(198, 86, 72),
        RuntimePathKind::MemoryRelease => Color32::from_rgb(96, 162, 116),
        RuntimePathKind::RefactorPreview => Color32::from_rgb(98, 204, 110),
    }
}

fn node_color(node: &Node3D) -> Color32 {
    if node.heat > 0.7 {
        return Color32::from_rgb(227, 191, 78);
    }
    let lower = node.kind.to_ascii_lowercase();
    if lower.contains("core") {
        Color32::from_rgb(74, 120, 202)
    } else if lower.contains("infra") {
        Color32::from_rgb(196, 88, 72)
    } else if lower.contains("interface") {
        Color32::from_rgb(196, 196, 196)
    } else {
        Color32::from_rgb(88, 164, 108)
    }
}

fn layer_for_x(x: f32) -> usize {
    if x <= 5.0 {
        0
    } else if x <= 15.0 {
        1
    } else if x <= 25.0 {
        2
    } else {
        3
    }
}

fn synthesize_scene(ir: &StructureViewIR) -> Structure3DIr {
    let nodes = ir
        .nodes
        .iter()
        .map(|node| Node3D {
            id: node.id.clone(),
            label: node.label.clone(),
            kind: node.role.clone(),
            position: Vec3 {
                x: node.x,
                y: node.y,
                z: node.z,
            },
            size: 14.0,
            importance: 0.5,
            heat: 0.0,
            source_binding: None,
        })
        .collect::<Vec<_>>();
    let edges = ir
        .edges
        .iter()
        .map(|edge| crate::model::Edge3D {
            from: edge.from.clone(),
            to: edge.to.clone(),
            weight: 1.0,
            edge_kind: edge.kind.clone(),
            violation: edge.cycle,
        })
        .collect::<Vec<_>>();
    let layers = vec![
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
    ];
    let clusters = nodes
        .iter()
        .fold(BTreeMap::<String, Vec<String>>::new(), |mut acc, node| {
            acc.entry(node.kind.clone())
                .or_default()
                .push(node.id.clone());
            acc
        })
        .into_iter()
        .map(|(label, nodes)| Cluster3D {
            id: label.to_ascii_lowercase(),
            label,
            nodes,
            color: "blue".to_string(),
        })
        .collect::<Vec<_>>();
    let graph = SemanticGraph3D {
        nodes,
        edges,
        clusters,
        layers,
    };
    Structure3DIr {
        timeline: crate::model::Timeline3D {
            snapshots: vec![GraphSnapshot3D {
                label: "before".to_string(),
                tick: 0,
                animation: GraphDeltaAnimation::default(),
            }],
            current_tick: 0,
            autoplay: false,
        },
        graph,
        ..Default::default()
    }
}

fn apply_graph_animation(
    base: &SemanticGraph3D,
    animation: &GraphDeltaAnimation,
) -> SemanticGraph3D {
    let mut graph = base.clone();
    for moved in &animation.moved_nodes {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == moved.node_id) {
            node.position = moved.after;
        }
    }
    for edge in &animation.removed_edges {
        graph.edges.retain(|current| {
            !(current.from == edge.from && current.to == edge.to && current.edge_kind == edge.kind)
        });
    }
    for edge in &animation.added_edges {
        let exists = graph.edges.iter().any(|current| {
            current.from == edge.from && current.to == edge.to && current.edge_kind == edge.kind
        });
        if !exists {
            graph.edges.push(crate::model::Edge3D {
                from: edge.from.clone(),
                to: edge.to.clone(),
                weight: 1.0,
                edge_kind: edge.kind.clone(),
                violation: edge.violation_after,
            });
        }
    }
    for edge in &mut graph.edges {
        if let Some(delta) = animation
            .removed_edges
            .iter()
            .chain(animation.added_edges.iter())
            .find(|delta| {
                delta.from == edge.from && delta.to == edge.to && delta.kind == edge.edge_kind
            })
        {
            edge.violation = delta.violation_after;
        }
    }
    graph
}
