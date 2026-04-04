use crate::model::StructureViewIR;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub fn render(
    ui: &mut egui::Ui,
    ir: &StructureViewIR,
    search: &str,
    selected: &mut Option<String>,
) {
    // 利用可能領域全体を使用する
    let available = ui.available_size();
    let h = available.y.max(200.0);
    let desired = egui::vec2(available.x, h);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, Color32::from_rgb(248, 245, 238));

    if ir.nodes.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No nodes — run analyze first",
            FontId::proportional(14.0),
            Color32::from_gray(160),
        );
        return;
    }

    let bounds = NodeBounds::compute(ir);
    paint_layer_groups(&painter, rect, ir, &bounds);
    paint_edges(&painter, rect, ir, &bounds);
    paint_nodes(
        &painter,
        rect,
        ir,
        search,
        selected,
        response.interact_pointer_pos(),
        &bounds,
    );
}

/// ノード座標のバウンディングボックス（自動フィット用）
struct NodeBounds {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
}

impl NodeBounds {
    fn compute(ir: &StructureViewIR) -> Self {
        let x_min = ir.nodes.iter().map(|n| n.x).fold(f32::MAX, f32::min);
        let x_max = ir.nodes.iter().map(|n| n.x).fold(f32::MIN, f32::max);
        let y_min = ir.nodes.iter().map(|n| n.y).fold(f32::MAX, f32::min);
        let y_max = ir.nodes.iter().map(|n| n.y).fold(f32::MIN, f32::max);
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
        }
    }

    /// [0, 1] に正規化した x 座標を返す
    fn norm_x(&self, x: f32) -> f32 {
        let range = (self.x_max - self.x_min).max(1.0);
        (x - self.x_min) / range
    }

    /// [0, 1] に正規化した y 座標を返す
    fn norm_y(&self, y: f32) -> f32 {
        let range = (self.y_max - self.y_min).max(1.0);
        (y - self.y_min) / range
    }
}

/// 正規化座標 [0,1] を rect にマップする（マージン付き）
fn project_2d(rect: Rect, nx: f32, ny: f32) -> Pos2 {
    let margin_x = 60.0;
    let margin_y = 48.0;
    let usable_w = (rect.width() - margin_x * 2.0).max(1.0);
    let usable_h = (rect.height() - margin_y * 2.0).max(1.0);
    Pos2::new(
        rect.left() + margin_x + nx * usable_w,
        rect.top() + margin_y + ny * usable_h,
    )
}

fn paint_layer_groups(
    painter: &egui::Painter,
    rect: Rect,
    ir: &StructureViewIR,
    bounds: &NodeBounds,
) {
    let max_layer = ir.nodes.iter().map(|n| n.layer).max().unwrap_or_default() + 1;

    // 各レイヤーの y 正規化範囲から実際の帯位置を計算
    for layer in 0..max_layer {
        let nodes_in_layer: Vec<_> = ir.nodes.iter().filter(|n| n.layer == layer).collect();

        let (band_top, band_bot) = if nodes_in_layer.is_empty() {
            // ノードがなければ等分割
            let top = rect.top() + rect.height() * (layer as f32 / max_layer as f32);
            let bot = rect.top() + rect.height() * ((layer + 1) as f32 / max_layer as f32);
            (top, bot)
        } else {
            // そのレイヤーのノードの y 範囲 + マージンで帯を描く
            let ny_min = nodes_in_layer
                .iter()
                .map(|n| bounds.norm_y(n.y))
                .fold(f32::MAX, f32::min);
            let ny_max = nodes_in_layer
                .iter()
                .map(|n| bounds.norm_y(n.y))
                .fold(f32::MIN, f32::max);
            let margin_y = 48.0;
            let usable_h = (rect.height() - margin_y * 2.0).max(1.0);
            let pad = 28.0;
            (
                (rect.top() + margin_y + ny_min * usable_h - pad).max(rect.top()),
                (rect.top() + margin_y + ny_max * usable_h + pad).min(rect.bottom()),
            )
        };

        let band = Rect::from_min_max(
            Pos2::new(rect.left(), band_top),
            Pos2::new(rect.right(), band_bot),
        );
        let fill = if layer % 2 == 0 {
            Color32::from_rgba_unmultiplied(214, 225, 235, 45)
        } else {
            Color32::from_rgba_unmultiplied(235, 221, 204, 36)
        };
        painter.rect_filled(band, 0.0, fill);
        painter.text(
            Pos2::new(rect.left() + 10.0, band_top + 6.0),
            Align2::LEFT_TOP,
            format!("Layer {layer}"),
            FontId::proportional(12.0),
            Color32::from_gray(60),
        );
    }
}

fn paint_edges(painter: &egui::Painter, rect: Rect, ir: &StructureViewIR, bounds: &NodeBounds) {
    for edge in &ir.edges {
        let Some(from) = ir.nodes.iter().find(|n| n.id == edge.from) else {
            continue;
        };
        let Some(to) = ir.nodes.iter().find(|n| n.id == edge.to) else {
            continue;
        };
        let a = project_2d(rect, bounds.norm_x(from.x), bounds.norm_y(from.y));
        let b = project_2d(rect, bounds.norm_x(to.x), bounds.norm_y(to.y));
        let stroke = if edge.cycle {
            Stroke::new(2.0, Color32::from_rgb(196, 73, 61))
        } else {
            Stroke::new(1.0, Color32::from_gray(120))
        };
        painter.line_segment([a, b], stroke);
    }
}

fn paint_nodes(
    painter: &egui::Painter,
    rect: Rect,
    ir: &StructureViewIR,
    search: &str,
    selected: &mut Option<String>,
    pointer_pos: Option<Pos2>,
    bounds: &NodeBounds,
) {
    let needle = search.trim().to_lowercase();
    for node in &ir.nodes {
        let pos = project_2d(rect, bounds.norm_x(node.x), bounds.norm_y(node.y));
        let is_selected = selected.as_deref() == Some(node.id.as_str());
        let matches_search = needle.is_empty()
            || node.id.to_lowercase().contains(&needle)
            || node.label.to_lowercase().contains(&needle);

        let radius = if is_selected { 18.0 } else { 13.0 };
        let fill = if is_selected {
            Color32::from_rgb(38, 118, 184)
        } else if !needle.is_empty() && matches_search {
            Color32::from_rgb(87, 160, 98)
        } else {
            Color32::from_gray(150)
        };

        painter.circle_filled(pos, radius, fill);
        // 選択ノードはリング追加
        if is_selected {
            painter.circle_stroke(
                pos,
                radius + 3.0,
                Stroke::new(1.5, Color32::from_rgb(38, 118, 184)),
            );
        }
        painter.text(
            pos + Vec2::new(0.0, radius + 5.0),
            Align2::CENTER_TOP,
            &node.label,
            FontId::proportional(11.0),
            Color32::from_gray(25),
        );

        if let Some(ptr) = pointer_pos {
            if ptr.distance(pos) <= radius {
                *selected = Some(node.id.clone());
            }
        }
    }
}
