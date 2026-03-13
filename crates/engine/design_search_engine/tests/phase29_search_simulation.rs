use design_domain::{Architecture, Constraint, DesignUnit, Dependency, DependencyKind, DesignUnitId};
use design_search_engine::{
    BeamSearchController, SearchConfig, SearchController as _, SearchState, rank_candidates,
};
use world_model_core::{EvaluationVector, WorldState};

fn search_config(max_candidates: usize) -> SearchConfig {
    SearchConfig {
        max_depth: 1,
        max_candidates,
        beam_width: 4,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    }
}

fn state_with_dependency(constraints: Vec<Constraint>) -> WorldState {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::new(1, "ServiceA"));
    architecture.add_design_unit(DesignUnit::new(2, "DatabaseB"));
    architecture.dependencies.push(Dependency {
        from: DesignUnitId(1),
        to: DesignUnitId(2),
        kind: DependencyKind::Calls,
    });
    architecture.graph.edges.push((1, 2));
    WorldState::from_architecture(1, architecture, constraints)
}

#[test]
fn phase29_search_propagates_simulation_result_into_evaluation_vector() {
    let controller = BeamSearchController::default();
    let states = controller.search(WorldState::new(1, vec![2.0, 1.0]), None, &search_config(4));

    assert!(!states.is_empty());
    for state in states {
        let simulation = state.world_state.simulation.as_ref().expect("simulation");
        assert_eq!(state.world_state.evaluation.simulation_quality, simulation.total());
        assert!(state.evaluation_result.is_some());
    }
}

#[test]
fn phase29_constraint_violations_reduce_search_score() {
    let controller = BeamSearchController::default();
    let unconstrained = controller.search(state_with_dependency(Vec::new()), None, &search_config(1));
    let constrained = controller.search(
        state_with_dependency(vec![Constraint {
            name: "max_two_units".into(),
            max_design_units: Some(2),
            max_dependencies: Some(1),
        }]),
        None,
        &search_config(1),
    );

    assert_eq!(unconstrained.len(), 1);
    assert_eq!(constrained.len(), 1);
    assert!(constrained[0].score < unconstrained[0].score);
}

#[test]
fn phase29_ranking_prefers_higher_simulation_feasibility() {
    let mut feasible = SearchState::new(1, WorldState::new(1, vec![2.0, 1.0]));
    feasible.world_state.evaluation = EvaluationVector {
        structural_quality: 0.8,
        dependency_quality: 0.8,
        constraint_satisfaction: 0.8,
        complexity: 0.2,
        simulation_quality: 0.9,
    };
    feasible.score = 0.5;

    let mut infeasible = SearchState::new(2, WorldState::new(2, vec![2.0, 1.0]));
    infeasible.world_state.evaluation = EvaluationVector {
        structural_quality: 0.8,
        dependency_quality: 0.8,
        constraint_satisfaction: 0.8,
        complexity: 0.2,
        simulation_quality: 0.1,
    };
    infeasible.score = 0.5;

    let ranked = rank_candidates(vec![infeasible, feasible]);

    assert_eq!(ranked[0].state.state_id, 1);
    assert_eq!(ranked[0].pareto_rank, 0);
    assert!(ranked[1].pareto_rank > ranked[0].pareto_rank);
}
