use std::time::Instant;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};

use crate::model::{PreviewGraph, StructureViewIR};

pub fn render(ui: &mut egui::Ui, ir: &StructureViewIR, animation_start: Instant) {
    ui.heading("Before / After Preview");
    let Some(preview) = &ir.preview else {
        ui.label("No preview overlay is currently attached to the IR.");
        return;
    };

    let elapsed = animation_start.elapsed().as_secs_f32();
    let transition = (elapsed.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    ui.label(format!("Transition {:.0}% -> after", transition * 100.0));

    ui.columns(2, |columns| {
        render_graph(
            &mut columns[0],
            "Before",
            &preview.before_graph,
            Color32::from_rgb(220, 126, 91),
        );
        render_graph(
            &mut columns[1],
            "After",
            &preview.after_graph,
            Color32::from_rgb(93, 154, 106),
        );
    });

    if !preview.changed_edges.is_empty() {
        ui.separator();
        ui.label("Changed edges");
        for edge in &preview.changed_edges {
            ui.label(format!("{} {} -> {}", edge.change, edge.from, edge.to));
        }
    }
    if !preview.moved_files.is_empty() {
        ui.separator();
        ui.label("Moved files");
        for moved in &preview.moved_files {
            ui.label(moved);
        }
    }
}

fn render_graph(ui: &mut egui::Ui, title: &str, graph: &PreviewGraph, accent: Color32) {
    ui.label(title);
    let desired = egui::vec2(ui.available_width(), 180.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, Color32::from_rgb(247, 244, 238));
    let count = graph.nodes.len().max(1) as f32;
    for (index, node) in graph.nodes.iter().enumerate() {
        let t = index as f32 / count;
        let pos = Pos2::new(
            rect.left() + 30.0 + t * (rect.width() - 60.0),
            rect.top() + 42.0 + ((index % 2) as f32 * 56.0),
        );
        painter.circle_filled(pos, 10.0, accent);
        painter.text(
            pos + egui::vec2(0.0, 16.0),
            Align2::CENTER_TOP,
            node,
            FontId::proportional(10.5),
            Color32::from_gray(40),
        );
    }
    for edge in &graph.edges {
        let Some(from_index) = graph.nodes.iter().position(|node| node == &edge.from) else {
            continue;
        };
        let Some(to_index) = graph.nodes.iter().position(|node| node == &edge.to) else {
            continue;
        };
        let from = node_pos(rect, from_index, count);
        let to = node_pos(rect, to_index, count);
        painter.line_segment([from, to], Stroke::new(1.2, accent));
    }
}

fn node_pos(rect: Rect, index: usize, count: f32) -> Pos2 {
    let t = index as f32 / count;
    Pos2::new(
        rect.left() + 30.0 + t * (rect.width() - 60.0),
        rect.top() + 42.0 + ((index % 2) as f32 * 56.0),
    )
}
