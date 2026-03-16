use architecture_evaluator::{ArchitectureEvaluatorEngine, ArchitectureIrEvaluator};
use architecture_ir::{
    ArchitectureConstraint, ArchitectureIR, ArchitectureMetadata, ComponentMetrics, ComponentType,
    ComponentUnit, ConstraintType, ConstraintValue, DependencyEdge, DependencyType, InterfaceUnit,
    Layer, NodeId, Visibility,
};
use memory_space_phase14::DesignMemorySpace;
use std::sync::{Arc, Mutex};

fn sample_ir() -> ArchitectureIR {
    ArchitectureIR {
        metadata: ArchitectureMetadata {
            ir_version: "v1".to_string(),
            version: "0.3".to_string(),
            language: Some("Rust".to_string()),
            created_at: 1,
            score: None,
            author: None,
            evaluation: None,
        },
        domains: vec![],
        components: vec![
            ComponentUnit {
                id: 1,
                name: "ApiController".to_string(),
                component_type: ComponentType::Controller,
                layer: Some(1),
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics {
                    fan_out: 1,
                    ..ComponentMetrics::default()
                },
            },
            ComponentUnit {
                id: 2,
                name: "UserService".to_string(),
                component_type: ComponentType::Service,
                layer: Some(2),
                interfaces: vec![10],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics {
                    fan_in: 1,
                    fan_out: 1,
                    ..ComponentMetrics::default()
                },
            },
            ComponentUnit {
                id: 3,
                name: "UserRepository".to_string(),
                component_type: ComponentType::Repository,
                layer: Some(3),
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics {
                    fan_in: 1,
                    ..ComponentMetrics::default()
                },
            },
        ],
        interfaces: vec![InterfaceUnit {
            id: 10,
            name: "UserPort".to_string(),
            input_types: vec!["Request".to_string()],
            output_types: vec!["User".to_string()],
            owner_component: 2,
        }],
        structures: vec![],
        design_units: vec![],
        dependencies: vec![
            DependencyEdge {
                source: NodeId::Component(1),
                target: NodeId::Component(2),
                dependency_type: DependencyType::Call,
                interface: None,
            },
            DependencyEdge {
                source: NodeId::Component(2),
                target: NodeId::Component(3),
                dependency_type: DependencyType::Use,
                interface: Some(10),
            },
        ],
        layers: vec![
            Layer {
                id: 1,
                name: "Presentation".to_string(),
                level: 3,
                components: vec![1],
                allowed_dependencies: vec![],
            },
            Layer {
                id: 2,
                name: "Application".to_string(),
                level: 2,
                components: vec![2],
                allowed_dependencies: vec![],
            },
            Layer {
                id: 3,
                name: "Infrastructure".to_string(),
                level: 1,
                components: vec![3],
                allowed_dependencies: vec![],
            },
        ],
        constraints: vec![
            ArchitectureConstraint {
                constraint_type: ConstraintType::NoCircularDependency,
                description: "acyclic".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "layered".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
        ],
    }
}

#[test]
fn evaluator_is_deterministic_and_uses_cache() {
    let evaluator = ArchitectureEvaluatorEngine::default();
    let ir = sample_ir();

    let first = evaluator.evaluate_ir(&ir);
    let second = evaluator.evaluate_ir(&ir);

    assert_eq!(first.scores, second.scores);
    assert!(!first.telemetry.cache_hit);
    assert!(second.telemetry.cache_hit);
    assert_eq!(evaluator.cache_size(), 1);
}

#[test]
fn evaluator_detects_layer_violation_and_cycle() {
    let evaluator = ArchitectureEvaluatorEngine::default();
    let mut ir = sample_ir();
    ir.dependencies.push(DependencyEdge {
        source: NodeId::Component(3),
        target: NodeId::Component(2),
        dependency_type: DependencyType::Use,
        interface: None,
    });
    ir.dependencies.push(DependencyEdge {
        source: NodeId::Component(2),
        target: NodeId::Component(1),
        dependency_type: DependencyType::Use,
        interface: None,
    });

    let result = evaluator.evaluate_ir(&ir);

    assert!(result.metrics.cycle_count > 0);
    assert!(!result.diagnostics.layer_violations.is_empty());
    assert!(!result.diagnostics.circular_dependencies.is_empty());
    assert!(result.scores.layering_score < 1.0);
}

#[test]
fn evaluator_normalizes_scores_between_zero_and_one() {
    let evaluator = ArchitectureEvaluatorEngine::default();
    let result = evaluator.evaluate_ir(&sample_ir());

    assert!((0.0..=1.0).contains(&result.scores.layering_score));
    assert!((0.0..=1.0).contains(&result.scores.coupling_score));
    assert!((0.0..=1.0).contains(&result.scores.cohesion_score));
    assert!((0.0..=1.0).contains(&result.scores.complexity_score));
    assert!((0.0..=1.0).contains(&result.scores.modularity_score));
    assert!((0.0..=1.0).contains(&result.scores.overall_score));
}

#[test]
fn evaluator_reports_interface_mismatch() {
    let evaluator = ArchitectureEvaluatorEngine::default();
    let mut ir = sample_ir();
    ir.dependencies[1].interface = Some(999);

    let result = evaluator.evaluate_ir(&ir);

    assert!(!result.diagnostics.interface_mismatch.is_empty());
}

#[test]
fn evaluator_reuses_evaluation_memory_cache() {
    let memory = Arc::new(Mutex::new(DesignMemorySpace::default()));
    let first = ArchitectureEvaluatorEngine::with_memory_space(memory.clone());
    let second = ArchitectureEvaluatorEngine::with_memory_space(memory);
    let ir = sample_ir();

    let cold = first.evaluate_ir(&ir);
    let warm = second.evaluate_ir(&ir);

    assert!(!cold.telemetry.cache_hit);
    assert!(warm.telemetry.cache_hit);
}
