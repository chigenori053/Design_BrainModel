use serde::Serialize;

use super::{RefactorPlan, RefactorTarget, StructureEdge, StructureGraph};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefactorPreview {
    pub before_graph: StructureGraph,
    pub after_graph: StructureGraph,
    pub removed_cycle_edge: Option<StructureEdge>,
    pub moved_files: Vec<String>,
    pub estimated_layer_score: f32,
    pub cli_text_preview: String,
}

pub fn render_preview(plan: &RefactorPlan) -> RefactorPreview {
    let removed_cycle_edge = plan.removed_edges.first().cloned();
    let moved_files = plan
        .moved_files
        .iter()
        .map(|(from, to)| format!("{} -> {}", from.display(), to.display()))
        .collect::<Vec<_>>();
    let estimated_layer_score = (1.0 - (plan.after_graph.edges.len() as f32 * 0.01)).max(0.0);

    RefactorPreview {
        before_graph: plan.before_graph.clone(),
        after_graph: plan.after_graph.clone(),
        removed_cycle_edge,
        moved_files,
        estimated_layer_score,
        cli_text_preview: cli_text_preview(plan),
    }
}

fn cli_text_preview(plan: &RefactorPlan) -> String {
    match &plan.target {
        RefactorTarget::Cycle => {
            let before = plan
                .removed_edges
                .first()
                .map(|edge| format!("{} -> {} -> {}", edge.from, edge.to, edge.from))
                .unwrap_or_else(|| "cycle detected".to_string());
            let after = plan
                .removed_edges
                .first()
                .map(|edge| {
                    format!(
                        "{} -> {}_{}_interface -> {}",
                        edge.from, edge.from, edge.to, edge.to
                    )
                })
                .unwrap_or_else(|| "cycle resolved".to_string());
            format!("Before:\n{before}\n\nAfter:\n{after}")
        }
        RefactorTarget::ExtractInterface { from, to } => {
            format!("Before:\n{from} -> {to}\n\nAfter:\n{from} -> {from}_{to}_interface -> {to}")
        }
        RefactorTarget::RemoveDependency { from, to } => {
            format!("Before:\n{from} -> {to}\n\nAfter:\nremoved dependency")
        }
        RefactorTarget::ModuleSplit(module) => {
            format!("Before:\n{module}\n\nAfter:\n{module}_core + {module}_api")
        }
        RefactorTarget::MergeModule(modules) => format!(
            "Before:\n{}\n\nAfter:\n{}",
            modules.join(" + "),
            modules.join("_")
        ),
        RefactorTarget::LayerViolation(detail) => format!(
            "Before:\n{detail}\n\nAfter:\n{}",
            detail.replace("->", " -> interface -> ")
        ),
        RefactorTarget::RenameBoundary(module) => {
            format!("Before:\n{module}\n\nAfter:\n{module}_boundary")
        }
        RefactorTarget::IntroduceService(module) => {
            format!("Before:\n{module}\n\nAfter:\n{module} -> {module}_service")
        }
        RefactorTarget::FileMove(path) => format!(
            "Before:\n{}\n\nAfter:\nsrc/moved/{}",
            path.display(),
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("moved.rs")
        ),
    }
}
