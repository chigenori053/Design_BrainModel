use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};

use crate::model::StructureViewIR;

pub fn render(ui: &mut egui::Ui, ir: &StructureViewIR, selected: &mut Option<String>) {
    ui.label(format!("3D backend: {:?}", wgpu::Backends::PRIMARY));
    let desired = egui::vec2(ui.available_width(), 360.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 12.0, Color32::from_rgb(241, 246, 249));

    let layer_count = ir
        .nodes
        .iter()
        .map(|node| node.layer)
        .max()
        .unwrap_or_default()
        + 1;
    for layer in 0..layer_count {
        let plane = plane_rect(rect, layer as f32);
        painter.rect_stroke(
            plane,
            10.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(65, 92, 120, 70)),
        );
        painter.text(
            Pos2::new(plane.left() + 8.0, plane.top() + 8.0),
            Align2::LEFT_TOP,
            format!("Architectural Layer {layer}"),
            FontId::proportional(11.0),
            Color32::from_gray(70),
        );
    }

    for edge in &ir.edges {
        let Some(from) = ir.nodes.iter().find(|node| node.id == edge.from) else {
            continue;
        };
        let Some(to) = ir.nodes.iter().find(|node| node.id == edge.to) else {
            continue;
        };
        let a = project_3d(rect, from.x, from.y, from.z);
        let b = project_3d(rect, to.x, to.y, to.z);
        painter.line_segment(
            [a, b],
            if edge.cycle {
                Stroke::new(2.0, Color32::from_rgb(201, 90, 64))
            } else {
                Stroke::new(1.0, Color32::from_gray(105))
            },
        );
    }

    for node in &ir.nodes {
        let pos = project_3d(rect, node.x, node.y, node.z);
        let is_selected = selected.as_deref() == Some(node.id.as_str());
        let radius = if is_selected { 15.0 } else { 11.0 };
        painter.circle_filled(
            pos,
            radius,
            if is_selected {
                Color32::from_rgb(38, 118, 184)
            } else {
                Color32::from_rgb(120, 155, 108)
            },
        );
        painter.text(
            pos + egui::vec2(0.0, radius + 8.0),
            Align2::CENTER_TOP,
            &node.label,
            FontId::proportional(11.0),
            Color32::from_gray(25),
        );
        if let Some(pointer) = response.interact_pointer_pos() {
            if pointer.distance(pos) <= radius {
                *selected = Some(node.id.clone());
            }
        }
    }
}

fn plane_rect(rect: Rect, layer: f32) -> Rect {
    let inset = 28.0 + layer * 22.0;
    Rect::from_min_max(
        Pos2::new(rect.left() + inset, rect.top() + inset * 0.65),
        Pos2::new(rect.right() - inset, rect.bottom() - inset * 0.35),
    )
}

fn project_3d(rect: Rect, x: f32, y: f32, z: f32) -> Pos2 {
    let depth = (z / 140.0).max(0.0);
    let scale = 1.0 - depth * 0.08;
    let px = rect.left() + rect.width() * 0.18 + x * 0.48 * scale + depth * 48.0;
    let py = rect.bottom() - rect.height() * 0.18 - y * 0.42 * scale - depth * 58.0;
    Pos2::new(
        px.clamp(rect.left() + 12.0, rect.right() - 12.0),
        py.clamp(rect.top() + 12.0, rect.bottom() - 12.0),
    )
}
