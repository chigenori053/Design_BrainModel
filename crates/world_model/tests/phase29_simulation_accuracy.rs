use design_domain::{Architecture, Constraint, DesignUnit, Dependency, DependencyKind, DesignUnitId};
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::WorldState;

fn architecture_with_edges(edges: &[(u64, u64)]) -> Architecture {
    let mut architecture = Architecture::seeded();
    for unit_id in 1..=3 {
        architecture.add_design_unit(DesignUnit::new(unit_id, format!("Unit{unit_id}")));
    }
    for (from, to) in edges {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(*from),
            to: DesignUnitId(*to),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((*from, *to));
    }
    architecture
}

#[test]
fn phase29_dependency_chain_is_reflected_in_simulation_metrics() {
    let state = WorldState::from_architecture(1, architecture_with_edges(&[(1, 2), (2, 3)]), Vec::new());
    let result = DefaultSimulationEngine.simulate(&state, None);

    assert_eq!(result.system.call_edges, 2);
    assert_eq!(result.system.dependency_cycles, 0);
    assert!(result.execution.dependency_cost > 0.0);
    assert!(result.performance_score > 0.0);
}

#[test]
fn phase29_cyclic_dependencies_reduce_logic_score() {
    let acyclic = WorldState::from_architecture(1, architecture_with_edges(&[(1, 2)]), Vec::new());
    let cyclic = WorldState::from_architecture(1, architecture_with_edges(&[(1, 2), (2, 1)]), Vec::new());

    let acyclic_result = DefaultSimulationEngine.simulate(&acyclic, None);
    let cyclic_result = DefaultSimulationEngine.simulate(&cyclic, None);

    assert!(cyclic_result.system.dependency_cycles > 0);
    assert!(cyclic_result.math.logic_score < acyclic_result.math.logic_score);
}

#[test]
fn phase29_constraint_violations_lower_constraint_score() {
    let architecture = architecture_with_edges(&[(1, 2)]);
    let baseline = WorldState::from_architecture(1, architecture.clone(), Vec::new());
    let constrained = WorldState::from_architecture(
        1,
        architecture,
        vec![Constraint {
            name: "no_dependencies".into(),
            max_design_units: None,
            max_dependencies: Some(0),
        }],
    );

    let baseline_result = DefaultSimulationEngine.simulate(&baseline, None);
    let constrained_result = DefaultSimulationEngine.simulate(&constrained, None);

    assert!(constrained_result.constraint_score < baseline_result.constraint_score);
}

#[test]
fn phase29_simulation_is_deterministic_for_identical_world_state() {
    let state = WorldState::from_architecture(42, architecture_with_edges(&[(1, 2), (2, 3)]), Vec::new());

    let left = DefaultSimulationEngine.simulate(&state, None);
    let right = DefaultSimulationEngine.simulate(&state, None);

    assert_eq!(left, right);
}
