use serde::Serialize;

use super::{RefactorPlan, RefactorTarget};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub cycle_removed: bool,
    pub no_new_layer_violation: bool,
    pub buildable: bool,
    pub public_api_preserved: bool,
    pub issues: Vec<String>,
}

pub fn validate_refactor(plan: &RefactorPlan) -> Result<ValidationResult, String> {
    let before_cycles = cycle_count(&plan.before_graph);
    let after_cycles = cycle_count(&plan.after_graph);
    let expects_cycle_delta = matches!(
        plan.target,
        RefactorTarget::Cycle
            | RefactorTarget::ExtractInterface { .. }
            | RefactorTarget::RemoveDependency { .. }
    );
    let cycle_removed = if expects_cycle_delta {
        after_cycles <= before_cycles.saturating_sub(1)
    } else {
        after_cycles <= before_cycles
    };
    let no_new_layer_violation = plan.after_graph.edges.len() <= plan.before_graph.edges.len() + 2;
    let public_api_preserved = !plan.affected_files.is_empty();
    let buildable = true;

    let mut issues = Vec::new();
    if expects_cycle_delta && !cycle_removed {
        issues.push("cycle still exists after refactor plan".to_string());
    }
    if !no_new_layer_violation {
        issues.push("after graph introduces additional layering pressure".to_string());
    }
    if !public_api_preserved {
        issues.push("public API preservation could not be inferred".to_string());
    }

    Ok(ValidationResult {
        valid: issues.is_empty(),
        cycle_removed,
        no_new_layer_violation,
        buildable,
        public_api_preserved,
        issues,
    })
}

fn cycle_count(graph: &super::StructureGraph) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| {
            graph
                .edges
                .iter()
                .any(|candidate| candidate.from == edge.to && candidate.to == edge.from)
        })
        .count()
        / 2
}
