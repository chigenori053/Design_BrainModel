use crate::model::StructureViewIR;

pub fn render(ui: &mut egui::Ui, ir: &StructureViewIR, selected: &mut Option<String>) {
    crate::render_3d::render(ui, ir, selected);
}
