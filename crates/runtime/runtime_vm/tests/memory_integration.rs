use std::sync::{Arc, Mutex};
use std::time::Instant;

use architecture_evaluator::{ArchitectureEvaluatorEngine, ArchitectureIrEvaluator};
use architecture_ir::{
    ArchitectureIR, ComponentMetrics, ComponentType, ComponentUnit, DependencyEdge, DependencyType,
    Layer, NodeId, Visibility,
};
use architecture_search::template::builtin_templates;
use architecture_search::template_engine::template_record_from_template;
use architecture_search::{
    ArchitectureSearchEngine, ArchitectureTemplateEngine, IntentModel, SearchConfig,
};
use memory_space_phase14::{
    ArchitectureMetadata, DesignMemorySpace, EvaluationDiagnostics, EvaluationMetricsV2,
    EvaluationScores, ReasoningTrace, SearchStep, embed_architecture, embed_evaluation,
    embed_template,
};
use runtime_vm::{ExecutionMode, dbm_test, test_support::with_test_vm};

fn seed_template_memory(memory: &mut DesignMemorySpace) {
    for template in builtin_templates().into_iter().take(5) {
        let mut record = template_record_from_template(&template);
        record.metadata.usage_count = if record.template_id == "microservice" {
            32
        } else {
            16
        };
        record.metadata.average_score = if record.template_id == "microservice" {
            1.0
        } else {
            0.95
        };
        record.metadata.success_rate = if record.template_id == "microservice" {
            1.0
        } else {
            0.9
        };
        memory.store_template(record.clone(), template_embedding(&record), &[]);
    }
}

fn template_embedding(record: &memory_space_phase14::TemplateRecord) -> Vec<f32> {
    match record.template_id.as_str() {
        "layered" => vec![1.0, 2.0, 0.0, 2.0],
        "hexagonal" => vec![0.95, 2.0, 0.0, 2.0],
        "microservice" => vec![0.9, 2.0, 0.0, 2.0],
        _ => embed_template(record),
    }
}

fn sample_ir(id: u64) -> ArchitectureIR {
    ArchitectureIR {
        components: vec![
            ComponentUnit {
                id: 1,
                name: format!("ApiController{id}"),
                component_type: ComponentType::Controller,
                layer: Some(1),
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            },
            ComponentUnit {
                id: 2,
                name: format!("Service{id}"),
                component_type: ComponentType::Service,
                layer: Some(2),
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            },
            ComponentUnit {
                id: 3,
                name: format!("Repository{id}"),
                component_type: ComponentType::Repository,
                layer: Some(3),
                interfaces: vec![],
                properties: vec![],
                structures: vec![],
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            },
        ],
        dependencies: vec![
            DependencyEdge {
                source: NodeId::Component(1),
                target: NodeId::Component(2),
                dependency_type: DependencyType::Use,
                interface: None,
            },
            DependencyEdge {
                source: NodeId::Component(2),
                target: NodeId::Component(3),
                dependency_type: DependencyType::Use,
                interface: None,
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
        ..ArchitectureIR::default()
    }
}

fn web_api_intent() -> IntentModel {
    IntentModel {
        system_type: "web_api".to_string(),
        requirements: vec!["caching".to_string(), "authentication".to_string()],
        ..IntentModel::default()
    }
}

dbm_test!(t1_recall_determinism_and_quality, #[ignore = "heavy memory integration"], runtime, {
    let mut memory = DesignMemorySpace::default();
    seed_template_memory(&mut memory);
    let engine = ArchitectureTemplateEngine::with_builtin_library();
    let intent = web_api_intent();

    let mut selected = Vec::new();
    for _ in 0..100 {
        let selection = engine.select_templates_with_memory(&intent, &memory);
        selected.push(selection.selected.template_id.clone());
        let recalled = memory
            .recall_templates_for_intent(
                &memory_space_phase14::DesignIntentRecord {
                    intent_id: "web_api".to_string(),
                    system_type: intent.system_type.clone(),
                    requirements: intent.requirements.clone(),
                    constraints: vec![],
                },
                3,
            )
            .into_iter()
            .map(|record| record.template_id)
            .collect::<Vec<_>>();
        let expected = ["layered", "hexagonal", "microservice"];
        let hits = expected
            .iter()
            .filter(|candidate| recalled.iter().any(|item| item == *candidate))
            .count();
        let accuracy = hits as f64 / expected.len() as f64;
        assert!(accuracy >= 0.8, "top_k recall accuracy was {accuracy:.2}");
    }

    assert!(selected.windows(2).all(|pair| pair[0] == pair[1]));
    let _ = &selected[0];
});

dbm_test!(t2_template_explosion_and_t6_learning_validation, #[ignore = "heavy memory integration"], runtime, {
    let _ = runtime;
    let engine = ArchitectureSearchEngine::default();
    let mut memory = DesignMemorySpace::default();
    seed_template_memory(&mut memory);
    let seed_count = memory.template_memory.all().len();

    for index in 0..1_000 {
        let mut intent = web_api_intent();
        if index % 3 == 0 {
            intent.requirements.push("observability".to_string());
        }
        if index % 5 == 0 {
            intent.requirements.push("queue".to_string());
        }
        let _ = engine.run_with_memory(&intent, &mut memory);
    }

    let template_count = memory.template_memory.all().len();
    let duplicate_free = memory
        .template_memory
        .all()
        .iter()
        .map(|record| record.template_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(
        template_count < 200,
        "template count grew to {template_count}"
    );
    assert_eq!(
        template_count, duplicate_free,
        "duplicate templates detected"
    );
    assert!(template_count >= seed_count);
});

dbm_test!(t3_memory_growth_and_consistency, #[ignore = "heavy memory integration"], runtime, {
    let _ = runtime;
    let mut memory = DesignMemorySpace::default();
    seed_template_memory(&mut memory);

    for index in 0..1_000 {
        let ir = sample_ir(index as u64);
        let architecture_id = format!("arch-{index}");
        let record = DesignMemorySpace::make_architecture_record(
            architecture_id.clone(),
            ir.clone(),
            "layered".to_string(),
            0.85,
            ArchitectureMetadata {
                search_depth: 3,
                generation_time: index as u64,
                search_iteration: index,
            },
        );
        memory.store_architecture(record.clone(), embed_architecture(&ir, 0.85));
        let evaluation = memory_space_phase14::EvaluationRecord {
            architecture_hash: format!("{:016x}", architecture_ir::architecture_hash(&ir)),
            evaluation_scores: EvaluationScores {
                overall_score: 0.85,
                ..EvaluationScores::default()
            },
            evaluation_metrics: EvaluationMetricsV2 {
                component_count: ir.components.len(),
                dependency_count: ir.dependencies.len(),
                layer_count: ir.layers.len(),
                cycle_count: 0,
                average_degree: ir.dependencies.len() as f64 / ir.components.len() as f64,
            },
            diagnostics: EvaluationDiagnostics::default(),
        };
        memory.store_evaluation(evaluation.clone(), embed_evaluation(&evaluation));
        memory.store_reasoning_trace(
            ReasoningTrace {
                trace_id: format!("trace-{index}"),
                intent: memory_space_phase14::DesignIntentRecord {
                    intent_id: format!("intent-{index}"),
                    system_type: "web_api".to_string(),
                    requirements: vec!["caching".to_string()],
                    constraints: vec![],
                },
                selected_template: "layered".to_string(),
                search_steps: vec![SearchStep {
                    step_id: 1,
                    action: "seed".to_string(),
                    score: "0.85".to_string(),
                }],
                candidate_architectures: vec![architecture_id.clone()],
                final_architecture: architecture_id,
            },
            vec![1.0, 0.85, 3.0],
        );
    }

    let node_count = memory.graph.nodes().len();
    let edge_count = memory.graph.edges().len();
    assert!(node_count < 10_000, "node_count={node_count}");
    for edge in memory.graph.edges() {
        assert!(memory.graph.get(edge.from).is_some(), "orphan from-node");
        assert!(memory.graph.get(edge.to).is_some(), "orphan to-node");
    }
    let _ = edge_count;
});

dbm_test!(t4_evaluation_cache_correctness, runtime, {
    let _ = runtime;
    let memory = Arc::new(Mutex::new(DesignMemorySpace::default()));
    let first = ArchitectureEvaluatorEngine::with_memory_space(memory.clone());
    let second = ArchitectureEvaluatorEngine::with_memory_space(memory);
    let architecture = sample_ir(42);

    let first_result = first.evaluate_ir(&architecture);
    let second_result = second.evaluate_ir(&architecture);

    assert_eq!(first_result.scores, second_result.scores);
    assert_eq!(first_result.metrics, second_result.metrics);
    assert_eq!(first_result.diagnostics, second_result.diagnostics);
    assert!(!first_result.telemetry.cache_hit);
    assert!(second_result.telemetry.cache_hit);
});

dbm_test!(t5_search_performance_improves_with_memory, #[ignore = "heavy memory integration"], runtime, {
    let _ = runtime;
    let engine = ArchitectureSearchEngine {
        config: SearchConfig {
            beam_width: 8,
            max_depth: 6,
            max_candidates: 1024,
            pareto_limit: 10,
            timeout_ms: 10_000,
        },
    };
    let intent = web_api_intent();
    let baseline_started = Instant::now();
    let baseline = engine.run(&intent);
    let baseline_time = baseline_started.elapsed();

    let mut memory = DesignMemorySpace::default();
    seed_template_memory(&mut memory);
    let memory_started = Instant::now();
    let guided = engine.run_with_memory(&intent, &mut memory);
    let memory_time = memory_started.elapsed();

    let candidate_reduction = 1.0
        - guided.telemetry.candidate_count as f64
            / baseline.telemetry.candidate_count.max(1) as f64;
    let evaluation_reduction = 1.0
        - guided.telemetry.generated_states as f64
            / baseline.telemetry.generated_states.max(1) as f64;

    assert!(
        candidate_reduction > 0.3,
        "candidate reduction was {candidate_reduction:.2}"
    );
    assert!(
        evaluation_reduction > 0.3,
        "evaluation reduction was {evaluation_reduction:.2}"
    );
    assert!(guided.telemetry.search_depth <= baseline.telemetry.search_depth);
    let _ = (baseline_time, memory_time);
});

dbm_test!(t7_integration_stability, #[ignore = "heavy memory integration"], runtime, {
    let mut panic_count = 0usize;
    let mut invalid_architecture = 0usize;
    let mut evaluation_failure = 0usize;

    for iteration in 0..1_000 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_test_vm(runtime, ExecutionMode::Reasoning, |vm| {
                vm.set_input_text(format!("design web api iteration {iteration}"));
                vm.execute();
                (
                    vm.context().design_state.is_some(),
                    vm.context().reasoning_result.is_some(),
                )
            })
        }));

        match result {
            Ok((has_design_state, has_reasoning_result)) => {
                if !has_design_state {
                    invalid_architecture += 1;
                }
                if !has_reasoning_result {
                    evaluation_failure += 1;
                }
            }
            Err(_) => panic_count += 1,
        }
    }

    assert_eq!(panic_count, 0);
    assert_eq!(invalid_architecture, 0);
    assert_eq!(evaluation_failure, 0);
});

dbm_test!(stress_memory_integration_10k_generations, #[ignore = "stress test"], runtime, {
    let _ = runtime;
    let engine = ArchitectureSearchEngine::default();
    let mut memory = DesignMemorySpace::default();
    seed_template_memory(&mut memory);
    let intent = web_api_intent();
    let started = Instant::now();

    for _ in 0..10_000 {
        let _ = engine.run_with_memory(&intent, &mut memory);
    }

    let recall_started = Instant::now();
    let _ = memory.recall_templates_for_intent(
        &memory_space_phase14::DesignIntentRecord {
            intent_id: "stress".to_string(),
            system_type: "web_api".to_string(),
            requirements: vec!["cache".to_string()],
            constraints: vec![],
        },
        3,
    );
    let recall_latency = recall_started.elapsed();
    assert!(recall_latency.as_millis() < 20, "recall latency too high");
    let _ = started;
});
