use super::{DesignGraph, MutationPlan, RationalityScore};

pub fn score_mutation_plan(graph: &DesignGraph, plan: &MutationPlan) -> RationalityScore {
    let cycle_risk = cycle_introduction_risk(graph, plan);
    let cohesion = crate_cohesion_preservation(plan);
    let inversion = dependency_inversion_correctness(plan);
    let blast_radius = public_api_blast_radius(plan);
    let ownership = ownership_responsibility_drift(plan);
    let rollback = rollback_unit_isolation(plan);

    let maintainability = (cohesion + inversion + rollback) / 3.0;
    let extensibility = (inversion + cohesion) / 2.0;
    let risk = 1.0 - ((cycle_risk + blast_radius + ownership) / 3.0);
    let boundary_integrity = ((1.0 - cycle_risk) + inversion + cohesion) / 3.0;
    let rollback_complexity = 1.0 - (1.0 - rollback);
    let total = weighted_total([
        1.0 - cycle_risk,
        cohesion,
        inversion,
        1.0 - blast_radius,
        1.0 - ownership,
        rollback,
    ]);

    RationalityScore {
        maintainability: clamp01(maintainability),
        extensibility: clamp01(extensibility),
        risk: clamp01(risk),
        boundary_integrity: clamp01(boundary_integrity),
        rollback_complexity: clamp01(rollback_complexity),
        total: clamp01(total),
    }
}

fn cycle_introduction_risk(graph: &DesignGraph, plan: &MutationPlan) -> f32 {
    let impacted = plan.delta.impacted_crates.len() as f32;
    let edges = graph.dependency_edges.len() as f32;
    clamp01((plan.delta.dependency_moves.len() as f32 * 0.18) + impacted * 0.07 + edges * 0.01)
}

fn crate_cohesion_preservation(plan: &MutationPlan) -> f32 {
    let impacted = plan.delta.impacted_crates.len() as f32;
    let interfaces = plan.delta.introduced_interfaces.len() as f32;
    clamp01(1.0 - ((impacted - 1.0).max(0.0) * 0.12) + (interfaces * 0.05))
}

fn dependency_inversion_correctness(plan: &MutationPlan) -> f32 {
    let interfaces = plan.delta.introduced_interfaces.len() as f32;
    let moves = plan.delta.dependency_moves.len() as f32;
    clamp01(0.68 + interfaces * 0.12 - moves * 0.03)
}

fn public_api_blast_radius(plan: &MutationPlan) -> f32 {
    let api_changes = plan.delta.api_changes.len() as f32;
    let targets = plan.target_files.len() as f32;
    clamp01(api_changes * 0.25 + (targets - 1.0).max(0.0) * 0.08)
}

pub fn blast_radius_inverse(plan: &MutationPlan) -> f32 {
    1.0 - public_api_blast_radius(plan)
}

fn ownership_responsibility_drift(plan: &MutationPlan) -> f32 {
    let impacted = plan.delta.impacted_crates.len() as f32;
    let rollback_units = plan.rollback_units.len() as f32;
    clamp01((impacted - rollback_units).abs() * 0.2 + impacted * 0.06)
}

fn rollback_unit_isolation(plan: &MutationPlan) -> f32 {
    let impacted = plan.delta.impacted_crates.len() as f32;
    let rollback_units = plan.rollback_units.len() as f32;
    clamp01(0.78 + rollback_units * 0.04 - (impacted - 1.0).max(0.0) * 0.1)
}

fn weighted_total(axes: [f32; 6]) -> f32 {
    let weights = [0.22, 0.16, 0.18, 0.16, 0.12, 0.16];
    axes.into_iter()
        .zip(weights)
        .map(|(axis, weight)| axis * weight)
        .sum()
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{CrateNode, DesignDelta, DesignGraph, MutationPlan};

    fn sample_graph() -> DesignGraph {
        DesignGraph {
            workspace_root: PathBuf::from("."),
            crates: vec![
                CrateNode {
                    name: "a".to_string(),
                    manifest_path: PathBuf::from("a/Cargo.toml"),
                    internal_dependencies: vec!["b".to_string()],
                },
                CrateNode {
                    name: "b".to_string(),
                    manifest_path: PathBuf::from("b/Cargo.toml"),
                    internal_dependencies: Vec::new(),
                },
            ],
            dependency_edges: vec![("a".to_string(), "b".to_string())],
        }
    }

    #[test]
    fn conservative_plan_scores_above_threshold() {
        let plan = MutationPlan {
            delta: DesignDelta {
                impacted_crates: vec!["a".to_string()],
                introduced_interfaces: vec!["a::Boundary".to_string()],
                ..DesignDelta::default()
            },
            target_files: vec![PathBuf::from("a/Cargo.toml")],
            expected_tests: vec!["cargo test -p a".to_string()],
            rollback_units: vec!["crate::a".to_string()],
        };
        let score = score_mutation_plan(&sample_graph(), &plan);
        assert!(score.total >= 0.82);
    }

    #[test]
    fn broad_api_plan_scores_lower() {
        let plan = MutationPlan {
            delta: DesignDelta {
                impacted_crates: vec!["a".to_string(), "b".to_string()],
                introduced_interfaces: vec!["a::Boundary".to_string()],
                dependency_moves: vec![("a".to_string(), "b".to_string())],
                api_changes: vec![
                    crate::design_delta::ApiChange {
                        crate_name: "a".to_string(),
                        surface: "public".to_string(),
                        change: "break".to_string(),
                    },
                    crate::design_delta::ApiChange {
                        crate_name: "b".to_string(),
                        surface: "public".to_string(),
                        change: "break".to_string(),
                    },
                ],
                ..DesignDelta::default()
            },
            target_files: vec![PathBuf::from("a/Cargo.toml"), PathBuf::from("b/Cargo.toml")],
            expected_tests: vec!["cargo test --workspace".to_string()],
            rollback_units: vec!["crate::a".to_string()],
        };
        let score = score_mutation_plan(&sample_graph(), &plan);
        assert!(score.total < 0.82);
    }
}
