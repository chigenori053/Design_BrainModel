use architecture_ir::{ArchitectureConstraint, ComponentType, ConstraintType};
use architecture_search::{
    ArchitectureEvaluator, BasicArchitectureEvaluator, BasicConstraintFilter, BeamSearchController,
    ConstraintFilter, DependencyRule, DesignIntent, DeterministicCandidateGenerator,
    ParetoSetOptimizer, SearchController, SearchSpace, create_initial_state,
};

fn search_space() -> SearchSpace {
    SearchSpace {
        component_catalog: vec![
            ComponentType::Controller,
            ComponentType::Service,
            ComponentType::Repository,
        ],
        allowed_dependencies: vec![
            DependencyRule {
                from: ComponentType::Controller,
                to: ComponentType::Service,
            },
            DependencyRule {
                from: ComponentType::Service,
                to: ComponentType::Repository,
            },
        ],
        constraints: vec![
            ArchitectureConstraint {
                constraint_type: ConstraintType::NoCircularDependency,
                description: "no cycles".to_string(),
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "respect layers".to_string(),
            },
        ],
    }
}

#[test]
fn determinism_test() {
    let space = search_space();
    let intent = DesignIntent {
        required_components: vec![
            ComponentType::Controller,
            ComponentType::Service,
            ComponentType::Repository,
        ],
    };
    let generator = DeterministicCandidateGenerator::new(space.clone(), intent);
    let filter = BasicConstraintFilter::new(space);
    let controller = BeamSearchController::new(
        4,
        4,
        generator,
        filter,
        BasicArchitectureEvaluator,
        ParetoSetOptimizer,
    );

    let mut initial = create_initial_state();
    initial.architecture.constraints = search_space().constraints;

    let first = controller.search(initial.clone());
    let second = controller.search(initial);

    assert_eq!(first, second);
}

#[test]
fn constraint_filtering_test() {
    let space = search_space();
    let filter = BasicConstraintFilter::new(space.clone());
    let evaluator = BasicArchitectureEvaluator;

    let mut valid = create_initial_state();
    valid.architecture.constraints = space.constraints.clone();
    valid
        .architecture
        .components
        .push(architecture_ir::ComponentUnit {
            id: 1,
            name: "Controller1".to_string(),
            component_type: ComponentType::Controller,
            structures: vec![],
            visibility: architecture_ir::Visibility::Public,
            metrics: architecture_ir::ComponentMetrics::default(),
        });
    valid
        .architecture
        .components
        .push(architecture_ir::ComponentUnit {
            id: 2,
            name: "Service1".to_string(),
            component_type: ComponentType::Service,
            structures: vec![],
            visibility: architecture_ir::Visibility::Public,
            metrics: architecture_ir::ComponentMetrics::default(),
        });
    valid.architecture.layers = vec![
        architecture_ir::Layer {
            name: "Presentation".to_string(),
            level: 3,
            components: vec![1],
            allowed_dependencies: vec![],
        },
        architecture_ir::Layer {
            name: "Application".to_string(),
            level: 2,
            components: vec![2],
            allowed_dependencies: vec![],
        },
    ];
    valid
        .architecture
        .dependencies
        .push(architecture_ir::DependencyEdge {
            source: architecture_ir::NodeId::Component(1),
            target: architecture_ir::NodeId::Component(2),
            dependency_type: architecture_ir::DependencyType::Use,
        });

    let mut invalid = valid.clone();
    invalid
        .architecture
        .dependencies
        .push(architecture_ir::DependencyEdge {
            source: architecture_ir::NodeId::Component(2),
            target: architecture_ir::NodeId::Component(1),
            dependency_type: architecture_ir::DependencyType::Use,
        });

    valid.score = evaluator.evaluate(&valid.architecture);
    invalid.score = evaluator.evaluate(&invalid.architecture);

    let filtered = filter.filter(vec![invalid, valid.clone()]);

    assert_eq!(filtered, vec![valid]);
}

#[test]
fn evaluation_stability_test() {
    let mut initial = create_initial_state();
    initial
        .architecture
        .components
        .push(architecture_ir::ComponentUnit {
            id: 1,
            name: "Service1".to_string(),
            component_type: ComponentType::Service,
            structures: vec![],
            visibility: architecture_ir::Visibility::Public,
            metrics: architecture_ir::ComponentMetrics {
                loc: 100,
                cyclomatic_complexity: 5,
                fan_in: 1,
                fan_out: 2,
            },
        });

    let evaluator = BasicArchitectureEvaluator;
    let left = evaluator.evaluate(&initial.architecture);
    let right = evaluator.evaluate(&initial.architecture);

    assert_eq!(left, right);
}

#[test]
fn search_convergence_test() {
    let space = search_space();
    let intent = DesignIntent {
        required_components: vec![
            ComponentType::Controller,
            ComponentType::Service,
            ComponentType::Repository,
        ],
    };
    let generator = DeterministicCandidateGenerator::new(space.clone(), intent.clone());
    let filter = BasicConstraintFilter::new(space.clone());
    let controller = BeamSearchController::new(
        4,
        4,
        generator,
        filter,
        BasicArchitectureEvaluator,
        ParetoSetOptimizer,
    );

    let mut initial = create_initial_state();
    initial.architecture.constraints = space.constraints;
    let outcome = controller.search_with_telemetry(initial);

    assert!(!outcome.states.is_empty());
    assert!(outcome.telemetry.explored_states > 0);
    assert!(outcome.states.iter().any(|state| {
        intent.required_components.iter().all(|component_type| {
            state
                .architecture
                .components
                .iter()
                .any(|component| &component.component_type == component_type)
        })
    }));
}
