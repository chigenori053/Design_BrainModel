use std::time::Instant;

use egui::Color32;

use crate::model::StructureViewIR;

pub fn render(ui: &mut egui::Ui, ir: &StructureViewIR, animation_start: Instant) {
    ui.heading("Preview Diff");
    let Some(preview) = &ir.preview else {
        ui.label("No preview diff is currently attached to the IR.");
        return;
    };

    let elapsed = animation_start.elapsed().as_secs_f32();
    let transition = (elapsed.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    ui.label(format!("Confidence animation {:.0}%", transition * 100.0));
    ui.label(format!("Candidate: {}", preview.candidate_id));
    ui.label(format!("Summary: {}", preview.summary));
    ui.label(format!("Estimated effect: {}", preview.estimated_effect));

    let safe_label = if preview.safe { "Safe" } else { "Unsafe" };
    let safe_color = if preview.safe {
        Color32::from_rgb(93, 154, 106)
    } else {
        Color32::from_rgb(220, 126, 91)
    };
    ui.colored_label(safe_color, safe_label);

    if !preview.diff_lines.is_empty() {
        ui.separator();
        ui.label("Diff");
        for line in &preview.diff_lines {
            ui.monospace(line);
        }
    }
}
