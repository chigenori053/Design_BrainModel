use crate::model::StructureViewIR;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub fn render(
    ui: &mut egui::Ui,
    ir: &StructureViewIR,
    search: &str,
    selected: &mut Option<String>,
) {
    let desired = egui::vec2(ui.available_width(), 360.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 12.0, Color32::from_rgb(248, 245, 238));
    paint_layer_groups(&painter, rect, ir);
    paint_edges(&painter, rect, ir);
    paint_nodes(
        &painter,
        rect,
        ir,
        search,
        selected,
        response.interact_pointer_pos(),
    );
    paint_minimap(ui, ir, search, selected);
}

fn paint_layer_groups(painter: &egui::Painter, rect: Rect, ir: &StructureViewIR) {
    let max_layer = ir
        .nodes
        .iter()
        .map(|node| node.layer)
        .max()
        .unwrap_or_default()
        + 1;
    for layer in 0..max_layer {
        let top = rect.top() + rect.height() * (layer as f32 / max_layer as f32);
        let bottom = rect.top() + rect.height() * ((layer + 1) as f32 / max_layer as f32);
        let band = Rect::from_min_max(Pos2::new(rect.left(), top), Pos2::new(rect.right(), bottom));
        let fill = if layer % 2 == 0 {
            Color32::from_rgba_unmultiplied(214, 225, 235, 40)
        } else {
            Color32::from_rgba_unmultiplied(235, 221, 204, 32)
        };
        painter.rect_filled(band, 0.0, fill);
        painter.text(
            Pos2::new(rect.left() + 10.0, top + 8.0),
            Align2::LEFT_TOP,
            format!("Layer {layer}"),
            FontId::proportional(11.0),
            Color32::from_gray(70),
        );
    }
}

fn paint_edges(painter: &egui::Painter, rect: Rect, ir: &StructureViewIR) {
    for edge in &ir.edges {
        let Some(from) = ir.nodes.iter().find(|node| node.id == edge.from) else {
            continue;
        };
        let Some(to) = ir.nodes.iter().find(|node| node.id == edge.to) else {
            continue;
        };
        let from = project_2d(rect, from.x, from.y);
        let to = project_2d(rect, to.x, to.y);
        let stroke = if edge.cycle {
            Stroke::new(2.0, Color32::from_rgb(196, 73, 61))
        } else {
            Stroke::new(1.0, Color32::from_gray(110))
        };
        painter.line_segment([from, to], stroke);
    }
}

fn paint_nodes(
    painter: &egui::Painter,
    rect: Rect,
    ir: &StructureViewIR,
    search: &str,
    selected: &mut Option<String>,
    pointer_pos: Option<Pos2>,
) {
    let needle = search.trim().to_lowercase();
    for node in &ir.nodes {
        let pos = project_2d(rect, node.x, node.y);
        let matches = needle.is_empty()
            || node.id.to_lowercase().contains(&needle)
            || node.label.to_lowercase().contains(&needle);
        let is_selected = selected.as_deref() == Some(node.id.as_str());
        let radius = if is_selected { 18.0 } else { 14.0 };
        let fill = if is_selected {
            Color32::from_rgb(38, 118, 184)
        } else if matches {
            Color32::from_rgb(87, 140, 98)
        } else {
            Color32::from_gray(158)
        };
        painter.circle_filled(pos, radius, fill);
        painter.text(
            pos + Vec2::new(0.0, radius + 10.0),
            Align2::CENTER_TOP,
            &node.label,
            FontId::proportional(11.0),
            Color32::from_gray(30),
        );

        if let Some(pointer) = pointer_pos {
            if pointer.distance(pos) <= radius {
                *selected = Some(node.id.clone());
            }
        }
    }
}

fn paint_minimap(
    ui: &mut egui::Ui,
    ir: &StructureViewIR,
    search: &str,
    selected: &mut Option<String>,
) {
    ui.add_space(8.0);
    ui.label("Mini Map");
    let desired = egui::vec2(ui.available_width().min(220.0), 90.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, Color32::from_rgb(243, 240, 232));
    let needle = search.trim().to_lowercase();
    for node in &ir.nodes {
        let pos = project_2d(rect, node.x * 0.35, node.y * 0.2);
        let fill = if selected.as_deref() == Some(node.id.as_str()) {
            Color32::from_rgb(38, 118, 184)
        } else if needle.is_empty() || node.id.to_lowercase().contains(&needle) {
            Color32::from_rgb(87, 140, 98)
        } else {
            Color32::from_gray(170)
        };
        painter.circle_filled(pos, 4.0, fill);
        if let Some(pointer) = response.interact_pointer_pos() {
            if pointer.distance(pos) <= 6.0 {
                *selected = Some(node.id.clone());
            }
        }
    }
}

fn project_2d(rect: Rect, x: f32, y: f32) -> Pos2 {
    let px = rect.left() + 30.0 + x * 0.55;
    let py = rect.top() + 30.0 + y * 0.8;
    Pos2::new(
        px.clamp(rect.left() + 10.0, rect.right() - 10.0),
        py.clamp(rect.top() + 10.0, rect.bottom() - 10.0),
    )
}
