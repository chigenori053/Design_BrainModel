use architecture_ir::{
    AnalysisResult, ArchitectureAnalyzer, ArchitectureConstraint, ArchitectureIR,
    ArchitectureIRBuilder, ArchitectureMetadata, BasicArchitectureAnalyzer, ComponentMetrics,
    ComponentNode, ComponentProperty, ComponentType, ComponentUnit, ConstraintType,
    ConstraintValue, DependencyEdge, DependencyType, DesignUnit, DomainUnit, InterfaceUnit, Layer,
    LayerRule, NodeId, RiskLevel, SemanticType, SourceLocation, StructureType, StructureUnit,
    ValidationError, ValidationWarning, Visibility, architecture_hash, export_dot, validate_ir,
};

fn sample_ir() -> ArchitectureIR {
    ArchitectureIR {
        metadata: ArchitectureMetadata {
            ir_version: "v1".to_string(),
            version: "0.3".to_string(),
            language: Some("Rust".to_string()),
            created_at: 1_742_000_000,
            score: Some(87),
            author: Some("DesignBrainModel".to_string()),
            evaluation: Some("healthy".to_string()),
        },
        domains: vec![DomainUnit {
            id: 10,
            name: "UserDomain".to_string(),
            components: vec![1, 2, 3],
        }],
        components: vec![
            ComponentUnit {
                id: 1,
                name: "ApiController".to_string(),
                component_type: ComponentType::Controller,
                layer: Some(1),
                interfaces: vec![],
                properties: vec![ComponentProperty {
                    key: "protocol".to_string(),
                    value: "http".to_string(),
                }],
                structures: vec![101],
                visibility: Visibility::Public,
                metrics: ComponentMetrics {
                    loc: 120,
                    cyclomatic_complexity: 8,
                    fan_in: 0,
                    fan_out: 2,
                },
            },
            ComponentUnit {
                id: 2,
                name: "UserService".to_string(),
                component_type: ComponentType::Service,
                layer: Some(2),
                interfaces: vec![501],
                properties: vec![],
                structures: vec![102],
                visibility: Visibility::Public,
                metrics: ComponentMetrics {
                    loc: 220,
                    cyclomatic_complexity: 11,
                    fan_in: 1,
                    fan_out: 1,
                },
            },
            ComponentUnit {
                id: 3,
                name: "UserRepository".to_string(),
                component_type: ComponentType::Repository,
                layer: Some(3),
                interfaces: vec![],
                properties: vec![],
                structures: vec![103],
                visibility: Visibility::Internal,
                metrics: ComponentMetrics {
                    loc: 140,
                    cyclomatic_complexity: 6,
                    fan_in: 1,
                    fan_out: 0,
                },
            },
        ],
        interfaces: vec![InterfaceUnit {
            id: 501,
            name: "UserRepositoryPort".to_string(),
            input_types: vec!["CreateUser".to_string()],
            output_types: vec!["User".to_string()],
            owner_component: 2,
        }],
        structures: vec![
            StructureUnit {
                id: 101,
                name: "handle_request".to_string(),
                structure_type: StructureType::Method,
                design_units: vec![1001],
            },
            StructureUnit {
                id: 102,
                name: "create_user".to_string(),
                structure_type: StructureType::Function,
                design_units: vec![1002],
            },
            StructureUnit {
                id: 103,
                name: "save_user".to_string(),
                structure_type: StructureType::Method,
                design_units: vec![1003],
            },
        ],
        design_units: vec![
            DesignUnit {
                id: 1001,
                semantic_type: SemanticType::Statement,
                source: SourceLocation {
                    file: "src/api.rs".to_string(),
                    line: 10,
                },
            },
            DesignUnit {
                id: 1002,
                semantic_type: SemanticType::Expression,
                source: SourceLocation {
                    file: "src/service.rs".to_string(),
                    line: 24,
                },
            },
            DesignUnit {
                id: 1003,
                semantic_type: SemanticType::Variable,
                source: SourceLocation {
                    file: "src/repository.rs".to_string(),
                    line: 30,
                },
            },
        ],
        dependencies: vec![
            DependencyEdge {
                source: NodeId::Domain(10),
                target: NodeId::Component(2),
                dependency_type: DependencyType::Use,
                interface: None,
            },
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
                interface: Some(501),
            },
            DependencyEdge {
                source: NodeId::Structure(101),
                target: NodeId::Structure(102),
                dependency_type: DependencyType::Call,
                interface: None,
            },
        ],
        layers: vec![
            Layer {
                id: 1,
                name: "Presentation".to_string(),
                level: 3,
                components: vec![1],
                allowed_dependencies: vec![LayerRule {
                    from: ComponentType::Controller,
                    to: ComponentType::Service,
                }],
            },
            Layer {
                id: 2,
                name: "Application".to_string(),
                level: 2,
                components: vec![2],
                allowed_dependencies: vec![LayerRule {
                    from: ComponentType::Service,
                    to: ComponentType::Repository,
                }],
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
                description: "Component dependency graph must stay acyclic.".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "Dependencies must only point inward.".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
        ],
    }
}

#[test]
fn validates_and_builds_petgraph_from_ir() {
    let ir = sample_ir();

    ir.validate().expect("sample IR should be valid");
    assert!(validate_ir(&ir).is_valid());
    let graph = ir.to_graph();

    assert_eq!(graph.node_count(), 7);
    assert_eq!(graph.edge_count(), 4);
}

#[test]
fn analyzer_reports_healthy_metrics_for_layered_ir() {
    let ir = sample_ir();

    let result: AnalysisResult = BasicArchitectureAnalyzer.analyze(&ir);

    assert!(result.risks.is_empty());
    assert_eq!(result.metrics.coupling, 1.0);
    assert_eq!(result.metrics.layering_score, 1.0);
    assert_eq!(result.metrics.complexity_score, 25.0 / 3.0);
}

#[test]
fn analyzer_detects_cycles_and_layer_violations() {
    let mut ir = sample_ir();
    ir.dependencies.push(DependencyEdge {
        source: NodeId::Component(3),
        target: NodeId::Component(1),
        dependency_type: DependencyType::Call,
        interface: None,
    });

    let result = BasicArchitectureAnalyzer.analyze(&ir);

    assert_eq!(result.risks.len(), 2);
    assert!(
        result
            .risks
            .iter()
            .any(|risk| risk.severity == RiskLevel::Critical)
    );
    assert!(result.metrics.layering_score < 1.0);
}

#[test]
fn validation_rejects_duplicate_component_ids() {
    let mut ir = sample_ir();
    ir.components[2].id = 2;

    let err = ir.validate().expect_err("duplicate ids must be rejected");
    assert!(matches!(err, ValidationError::DuplicateId));
}

#[test]
fn legacy_component_node_maps_to_component_unit() {
    let node = ComponentNode {
        id: 7,
        name: "LegacyController".to_string(),
        component_type: ComponentType::Controller,
        layer: None,
        interfaces: vec![],
        properties: vec![],
        visibility: Visibility::Public,
        metrics: ComponentMetrics {
            loc: 10,
            cyclomatic_complexity: 2,
            fan_in: 0,
            fan_out: 1,
        },
    };

    let unit: ComponentUnit = node.into();

    assert_eq!(unit.id, 7);
    assert!(unit.structures.is_empty());
}

#[test]
fn ir_build_test() {
    let ir = ArchitectureIRBuilder::new(ArchitectureMetadata::default())
        .add_component(1, "Controller1", ComponentType::Controller)
        .add_component(2, "Service1", ComponentType::Service)
        .add_structure(101, "handle", StructureType::Method)
        .attach_structure_to_component(1, 101)
        .add_dependency(
            NodeId::Component(1),
            NodeId::Component(2),
            DependencyType::Use,
        )
        .build();

    assert_eq!(ir.components.len(), 2);
    assert_eq!(ir.component_structures(1), vec![101]);
}

#[test]
fn query_api_test() {
    let ir = sample_ir();
    assert_eq!(ir.components().len(), 3);
    assert_eq!(ir.interfaces().len(), 1);
    assert_eq!(ir.component_dependencies(1), vec![2]);
    assert_eq!(ir.component_dependents(3), vec![2]);
    assert_eq!(ir.component_structures(2), vec![102]);
    assert_eq!(ir.component_interfaces(2), vec![501]);
}

#[test]
fn serialization_test() {
    let ir = sample_ir();
    let json = serde_json::to_string(&ir).expect("ir should serialize");
    let restored: ArchitectureIR = serde_json::from_str(&json).expect("ir should deserialize");
    assert_eq!(restored, ir);
}

#[test]
fn graph_export_and_hash_are_stable() {
    let ir = sample_ir();
    let left = architecture_hash(&ir);
    let right = architecture_hash(&ir);
    let dot = export_dot(&ir);

    assert_eq!(left, right);
    assert!(dot.contains("digraph"));
}

#[test]
fn validation_reports_domain_and_layer_issues() {
    let mut ir = sample_ir();
    ir.domains[0].components.push(99);
    ir.dependencies.push(DependencyEdge {
        source: NodeId::Component(3),
        target: NodeId::Component(2),
        dependency_type: DependencyType::Use,
        interface: None,
    });

    let result = validate_ir(&ir);

    assert!(result.errors.contains(&ValidationError::LayerViolation));
    assert!(
        result
            .warnings
            .contains(&ValidationWarning::DomainViolation)
    );
}

#[test]
fn mutation_api_supports_move_split_merge() {
    let mut ir = sample_ir();
    ir.move_layer(1, 2);
    assert_eq!(
        ir.component_by_id(1).and_then(|component| component.layer),
        Some(2)
    );

    ir.split_component(
        2,
        vec![
            ComponentUnit {
                id: 20,
                name: "AuthService".to_string(),
                component_type: ComponentType::Service,
                layer: None,
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            },
            ComponentUnit {
                id: 21,
                name: "UserService".to_string(),
                component_type: ComponentType::Service,
                layer: None,
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            },
        ],
    )
    .expect("split should succeed");
    assert!(ir.component_by_id(20).is_some());
    assert!(ir.component_by_id(21).is_some());

    ir.merge_components(
        &[20, 21],
        ComponentUnit {
            id: 30,
            name: "MergedService".to_string(),
            component_type: ComponentType::Service,
            layer: None,
            interfaces: vec![],
            properties: vec![],
            structures: vec![],
            visibility: Visibility::Public,
            metrics: ComponentMetrics::default(),
        },
    )
    .expect("merge should succeed");
    assert!(ir.component_by_id(30).is_some());
}

#[test]
fn validation_rejects_missing_interface_reference() {
    let mut ir = sample_ir();
    ir.components[1].interfaces.push(9999);

    let result = validate_ir(&ir);

    assert!(result.errors.contains(&ValidationError::MissingInterface));
}
