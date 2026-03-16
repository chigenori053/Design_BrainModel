use architecture_ir::{ArchitectureConstraint, ComponentType, ConstraintType};
use architecture_search::{
    ArchitectureEvaluator, ArchitectureGrammar, ArchitectureSearchEngine,
    ArchitectureTemplateEngine,
    BasicArchitectureEvaluator, BasicConstraintFilter, BeamSearchController, ConstraintFilter,
    ComponentRule, ConstraintRule, DependencyRule, DesignIntent,
    DeterministicCandidateGenerator, IntentConstraints, IntentModel, InterfaceRule, LayerRule,
    ParetoSetOptimizer, SearchConfig, SearchController, SearchSpace, create_initial_state,
};
use memory_space_phase14::DesignMemorySpace;

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
                value: None,
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "respect layers".to_string(),
                value: None,
            },
        ],
        forbidden_components: vec![],
        component_rules: vec![
            ComponentRule {
                name: "Controller".to_string(),
                component_type: ComponentType::Controller,
                layer: "Presentation".to_string(),
                allowed_dependencies: vec![ComponentType::Service],
                required_interfaces: vec![],
            },
            ComponentRule {
                name: "Service".to_string(),
                component_type: ComponentType::Service,
                layer: "Application".to_string(),
                allowed_dependencies: vec![ComponentType::Repository],
                required_interfaces: vec![],
            },
            ComponentRule {
                name: "Repository".to_string(),
                component_type: ComponentType::Repository,
                layer: "Infrastructure".to_string(),
                allowed_dependencies: vec![],
                required_interfaces: vec![],
            },
        ],
        layer_rules: vec![
            LayerRule {
                name: "Presentation".to_string(),
                level: 3,
                allowed_targets: vec!["Application".to_string()],
                contained_components: vec![ComponentType::Controller],
            },
            LayerRule {
                name: "Application".to_string(),
                level: 2,
                allowed_targets: vec!["Infrastructure".to_string()],
                contained_components: vec![ComponentType::Service],
            },
            LayerRule {
                name: "Infrastructure".to_string(),
                level: 1,
                allowed_targets: vec![],
                contained_components: vec![ComponentType::Repository],
            },
        ],
        interface_rules: Vec::<InterfaceRule>::new(),
        constraint_rule: ConstraintRule {
            max_dependencies_per_component: 5,
            no_circular_dependency: true,
            max_layer_depth: 4,
        },
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
        required_features: vec![],
        architectural_constraints: vec![],
    };
    let generator = DeterministicCandidateGenerator::new(space.clone(), intent);
    let filter = BasicConstraintFilter::new(space);
    let controller = BeamSearchController::new(
        SearchConfig {
            beam_width: 4,
            max_depth: 4,
            ..SearchConfig::default()
        },
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
            layer: Some(1),
            interfaces: vec![],
            properties: vec![],
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
            layer: Some(2),
            interfaces: vec![],
            properties: vec![],
            structures: vec![],
            visibility: architecture_ir::Visibility::Public,
            metrics: architecture_ir::ComponentMetrics::default(),
        });
    valid.architecture.layers = vec![
        architecture_ir::Layer {
            id: 1,
            name: "Presentation".to_string(),
            level: 3,
            components: vec![1],
            allowed_dependencies: vec![],
        },
        architecture_ir::Layer {
            id: 2,
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
            interface: None,
        });

    let mut invalid = valid.clone();
    invalid
        .architecture
        .dependencies
        .push(architecture_ir::DependencyEdge {
            source: architecture_ir::NodeId::Component(2),
            target: architecture_ir::NodeId::Component(1),
            dependency_type: architecture_ir::DependencyType::Use,
            interface: None,
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
            layer: None,
            interfaces: vec![],
            properties: vec![],
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
        required_features: vec![],
        architectural_constraints: vec![],
    };
    let generator = DeterministicCandidateGenerator::new(space.clone(), intent.clone());
    let filter = BasicConstraintFilter::new(space.clone());
    let controller = BeamSearchController::new(
        SearchConfig {
            beam_width: 4,
            max_depth: 4,
            ..SearchConfig::default()
        },
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

#[test]
fn grammar_prunes_invalid_layer_direction() {
    let grammar = ArchitectureGrammar::from_intent(&IntentModel {
        system_type: "web_api".to_string(),
        constraints: IntentConstraints {
            architecture: Some("layered".to_string()),
            ..IntentConstraints::default()
        },
        ..IntentModel::default()
    });
    let rules = grammar.dependency_rules();

    assert!(rules.iter().any(|rule| {
        rule.from == ComponentType::Controller && rule.to == ComponentType::Service
    }));
    assert!(!rules.iter().any(|rule| {
        rule.from == ComponentType::Repository && rule.to == ComponentType::Controller
    }));
}

#[test]
fn grammar_dsl_builds_component_and_layer_rules() {
    let grammar = ArchitectureGrammar::from_dsl(
        r#"
component Service
depends_on Repository
layer Application
contains Service
allows Infrastructure
layer Infrastructure
contains Repository
"#,
    )
    .expect("dsl should parse");

    assert!(grammar
        .component_rules
        .iter()
        .any(|rule| rule.component_type == ComponentType::Service));
    assert!(grammar
        .dependency_rules
        .iter()
        .any(|rule| rule.from == ComponentType::Service && rule.to == ComponentType::Repository));
    assert!(grammar
        .layer_rules
        .iter()
        .any(|rule| rule.name == "Application"));
}

#[test]
fn grammar_engine_rejects_direct_controller_to_repository() {
    let grammar = ArchitectureGrammar::from_intent(&IntentModel {
        system_type: "web_api".to_string(),
        constraints: IntentConstraints {
            architecture: Some("layered".to_string()),
            ..IntentConstraints::default()
        },
        ..IntentModel::default()
    });

    let mut state = create_initial_state();
    state.architecture.components.push(architecture_ir::ComponentUnit {
        id: 1,
        name: "Controller1".to_string(),
        component_type: ComponentType::Controller,
        layer: Some(1),
        interfaces: vec![],
        properties: vec![],
        structures: vec![],
        visibility: architecture_ir::Visibility::Public,
        metrics: architecture_ir::ComponentMetrics::default(),
    });
    state.architecture.components.push(architecture_ir::ComponentUnit {
        id: 2,
        name: "Repository1".to_string(),
        component_type: ComponentType::Repository,
        layer: Some(3),
        interfaces: vec![],
        properties: vec![],
        structures: vec![],
        visibility: architecture_ir::Visibility::Public,
        metrics: architecture_ir::ComponentMetrics::default(),
    });
    state.architecture.dependencies.push(architecture_ir::DependencyEdge {
        source: architecture_ir::NodeId::Component(1),
        target: architecture_ir::NodeId::Component(2),
        dependency_type: architecture_ir::DependencyType::Use,
        interface: None,
    });

    let validation = architecture_search::ArchitectureGrammarEngine.validate(
        &state.architecture,
        &grammar,
    );

    assert!(!validation.valid);
    assert!(validation
        .issues
        .iter()
        .any(|issue| issue.contains("forbidden dependency")));
}

#[test]
fn engine_returns_pareto_architectures_for_web_api_intent() {
    let engine = ArchitectureSearchEngine {
        config: SearchConfig {
            beam_width: 6,
            max_depth: 6,
            pareto_limit: 10,
            ..SearchConfig::default()
        },
    };
    let intent = IntentModel {
        system_type: "web_api".to_string(),
        requirements: vec![
            "authentication".to_string(),
            "logging".to_string(),
            "caching".to_string(),
        ],
        constraints: IntentConstraints {
            architecture: Some("layered".to_string()),
            language: Some("rust".to_string()),
            forbidden_components: vec![],
        },
        quality_attributes: vec!["performance".to_string(), "maintainability".to_string()],
        domain_context: vec!["api gateway".to_string()],
    };

    let result = engine.run(&intent);

    assert!(!result.candidates.is_empty());
    assert!(result.telemetry.explored_states > 0);
    assert!(result.pareto_frontier.iter().all(|candidate| {
        candidate
            .architecture_ir
            .components
            .iter()
            .all(|component| !intent.constraints.forbidden_components.contains(&component.component_type))
    }));
    assert!(result.pareto_frontier.iter().any(|candidate| {
        let component_types = candidate
            .architecture_ir
            .components
            .iter()
            .map(|component| component.component_type.clone())
            .collect::<Vec<_>>();
        component_types.contains(&ComponentType::Controller)
            && component_types.contains(&ComponentType::Service)
            && component_types.contains(&ComponentType::Repository)
    }));
    assert_eq!(
        result
            .template_selection
            .as_ref()
            .map(|selection| selection.selected.template_id.as_str()),
        Some("layered")
    );
}

#[test]
fn pareto_frontier_respects_limit_and_is_repeatable() {
    let engine = ArchitectureSearchEngine {
        config: SearchConfig {
            beam_width: 6,
            max_depth: 6,
            pareto_limit: 2,
            ..SearchConfig::default()
        },
    };
    let intent = IntentModel {
        system_type: "web_api".to_string(),
        requirements: vec!["authentication".to_string(), "caching".to_string()],
        constraints: IntentConstraints {
            architecture: Some("layered".to_string()),
            ..IntentConstraints::default()
        },
        ..IntentModel::default()
    };

    let left = engine.run(&intent);
    let right = engine.run(&intent);

    assert_eq!(left.pareto_frontier.len(), 2);
    assert_eq!(left.pareto_frontier, right.pareto_frontier);
}

#[test]
fn template_engine_selects_pipeline_for_data_pipeline_intent() {
    let engine = ArchitectureTemplateEngine::with_builtin_library();
    let selection = engine.select_templates(&IntentModel {
        system_type: "data_pipeline".to_string(),
        requirements: vec!["stream ingestion".to_string()],
        ..IntentModel::default()
    });

    assert_eq!(selection.selected.template_id, "pipeline");
}

#[test]
fn template_engine_mutates_layered_template_for_cache_and_auth() {
    let engine = ArchitectureTemplateEngine::with_builtin_library();
    let selection = engine.select_templates(&IntentModel {
        system_type: "web_api".to_string(),
        ..IntentModel::default()
    });
    let mutated = engine.mutate_template(
        &selection.selected,
        &IntentModel {
            system_type: "web_api".to_string(),
            requirements: vec!["authentication".to_string(), "caching".to_string()],
            ..IntentModel::default()
        },
    );

    assert!(mutated
        .component_slots
        .iter()
        .any(|slot| slot.slot_name == "AuthService"));
    assert!(mutated
        .component_slots
        .iter()
        .any(|slot| slot.slot_name == "CacheAdapter"));
}

#[test]
fn template_expansion_creates_partial_architecture_seed() {
    let engine = ArchitectureTemplateEngine::with_builtin_library();
    let selection = engine.select_templates(&IntentModel {
        system_type: "web_api".to_string(),
        ..IntentModel::default()
    });
    let seed = engine.expand_template(&selection.selected, &search_space());

    assert!(!seed.architecture.layers.is_empty());
    assert!(!seed.architecture.components.is_empty());
    assert!(seed
        .architecture
        .components
        .iter()
        .any(|component| component.name.contains("Service")));
}

#[test]
fn run_with_memory_persists_reasoning_trace_and_architectures() {
    let engine = ArchitectureSearchEngine::default();
    let mut memory = DesignMemorySpace::default();
    let intent = IntentModel {
        system_type: "web_api".to_string(),
        requirements: vec!["caching".to_string()],
        ..IntentModel::default()
    };

    let result = engine.run_with_memory(&intent, &mut memory);

    assert!(!result.candidates.is_empty());
    assert!(!memory.reasoning_trace_memory.all().is_empty());
    assert!(!memory.architecture_memory.all().is_empty());
}
