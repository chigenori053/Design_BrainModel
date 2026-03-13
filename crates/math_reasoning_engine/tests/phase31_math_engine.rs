use architecture_domain::ArchitectureState;
use design_domain::{Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer};
use math_reasoning_engine::{
    ComplexityClass, ConstraintSolver, DefaultMathematicalReasoningEngine,
    DeterministicConstraintSolver, DeterministicNumericalValidator, DeterministicSymbolicReasoner,
    HeuristicComplexityEstimator, MathEngine, MathematicalReasoningEngine, NumericalValidator,
    SymbolicReasoner, ComplexityEstimator, GraphAnalysisEngine, DeterministicGraphAnalysisEngine,
    architecture_search_step,
};
use world_model::{ArchitectureAction, DesignAction, WorldModel};

fn architecture_state(
    nodes: usize,
    dependencies: &[(u64, u64)],
    constraints: Vec<Constraint>,
) -> ArchitectureState {
    let mut architecture = Architecture::seeded();
    for id in 1..=nodes as u64 {
        let layer = match id % 4 {
            1 => Layer::Ui,
            2 => Layer::Service,
            3 => Layer::Repository,
            _ => Layer::Database,
        };
        architecture.add_design_unit(DesignUnit::with_layer(id, format!("Node{id}"), layer));
    }
    for (from, to) in dependencies {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(*from),
            to: DesignUnitId(*to),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((*from, *to));
    }
    ArchitectureState::from_architecture(&architecture, constraints)
}

#[test]
fn test34_01_constraint_solving() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let valid = architecture_state(3, &[(1, 2)], vec![Constraint {
        name: "deps".into(),
        max_design_units: Some(4),
        max_dependencies: Some(2),
    }]);
    let invalid = architecture_state(3, &[(1, 2), (2, 3)], vec![Constraint {
        name: "deps".into(),
        max_design_units: Some(3),
        max_dependencies: Some(1),
    }]);
    let solver = DeterministicConstraintSolver;

    assert!(solver.solve(&engine.problem_from_architecture(&valid)).satisfied);
    assert!(!solver.solve(&engine.problem_from_architecture(&invalid)).satisfied);
}

#[test]
fn test34_02_complexity_estimation() {
    let estimator = HeuristicComplexityEstimator;
    let simple = architecture_state(3, &[(1, 2)], Vec::new());
    let dense = architecture_state(12, &[(1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7), (7, 8), (8, 9), (9, 10), (10, 11), (11, 12), (12, 1), (2, 10), (3, 11), (4, 12)], Vec::new());

    assert_eq!(estimator.estimate(&simple).time_complexity, ComplexityClass::Constant);
    assert!(
        matches!(
            estimator.estimate(&dense).time_complexity,
            ComplexityClass::Linearithmic | ComplexityClass::Quadratic | ComplexityClass::Cubic
        )
    );
}

#[test]
fn test34_03_symbolic_reasoning() {
    let reasoner = DeterministicSymbolicReasoner;
    let architecture = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());
    let result = reasoner.reason(&architecture);

    assert!(result.summary.contains("latency = dependency_count"));
    assert!(result.validity_score > 0.0);
}

#[test]
fn test34_04_numerical_validation() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let validator = DeterministicNumericalValidator;
    let architecture = architecture_state(4, &[(1, 2)], Vec::new());
    let problem = engine.problem_from_architecture(&architecture);
    let result = validator.validate(&architecture, &problem);

    assert!(result.valid);
    assert!(result.stability_score > 0.0);
}

#[test]
fn test34_06_determinism() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let architecture = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());

    let left = engine.analyze_with_trace(&architecture);
    let right = engine.analyze_with_trace(&architecture);

    assert_eq!(left, right);
}

#[test]
fn test34_07_telemetry_completeness() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let architecture = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());
    let trace = engine.analyze_with_trace(&architecture);

    assert_eq!(trace.telemetry.len(), 4);
    assert!(trace.constraint_trace.checked_constraints > 0);
}

#[test]
fn test34_08_performance() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let architecture = architecture_state(16, &[(1, 2), (2, 3), (3, 4), (4, 5)], Vec::new());
    let started = std::time::Instant::now();
    for _ in 0..100 {
        let _ = engine.analyze(&architecture);
    }
    let average = started.elapsed() / 100;

    assert!(average.as_millis() < 20, "avg latency was {:?}", average);
}

#[test]
fn test34_09_graph_analysis_reports_depth_and_cycles() {
    let graph = DeterministicGraphAnalysisEngine;
    let architecture = architecture_state(4, &[(1, 2), (2, 3), (3, 1), (3, 4)], Vec::new());

    let metrics = graph.analyze(&architecture);

    assert_eq!(metrics.node_count, 4);
    assert_eq!(metrics.edge_count, 4);
    assert!(metrics.max_depth >= 2);
    assert_eq!(metrics.cycle_count, 1);
}

#[test]
fn test34_10_world_model_evaluation_is_deterministic() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let world = WorldModel::from_architecture(
        {
            let mut architecture = Architecture::seeded();
            architecture.add_design_unit(DesignUnit::with_layer(1, "ApiService", Layer::Service));
            architecture.add_design_unit(DesignUnit::with_layer(2, "Store", Layer::Database));
            architecture.dependencies.push(Dependency {
                from: DesignUnitId(1),
                to: DesignUnitId(2),
                kind: DependencyKind::Calls,
            });
            architecture.graph.edges.push((1, 2));
            architecture
        },
        Vec::new(),
    );

    let left = engine.evaluate(&world);
    let right = engine.evaluate(&world);

    assert_eq!(left, right);
}

#[test]
fn test34_11_evaluate_action_updates_scores_stably() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "ApiService", Layer::Service));
    let world = WorldModel::from_architecture(architecture, Vec::new());
    let action = DesignAction::Architecture(ArchitectureAction::AddComponent {
        component: DesignUnit::with_layer(2, "UserRepository", Layer::Repository),
    });

    let score = engine.evaluate_action(&world, &action);

    assert!((0.0..=1.0).contains(&score.performance));
    assert!((0.0..=1.0).contains(&score.complexity));
    assert!((0.0..=1.0).contains(&score.maintainability));
    assert!((0.0..=1.0).contains(&score.correctness));
}

#[test]
fn test34_12_search_compatibility_returns_ranked_action_scores() {
    let engine = DefaultMathematicalReasoningEngine::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "ApiService", Layer::Service));
    architecture.add_design_unit(DesignUnit::with_layer(2, "UserRepository", Layer::Repository));
    let world = WorldModel::from_architecture(architecture, Vec::new());

    let ranked = architecture_search_step(&world, &engine);

    assert!(!ranked.is_empty());
    assert!(ranked.iter().all(|(_, score)| (0.0..=1.0).contains(&score.correctness)));
}
