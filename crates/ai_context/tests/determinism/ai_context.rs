use ai_context::{AIContext, AIContextParts, EvaluationState, ExperienceState, RuntimeState};
use architecture_domain::ArchitectureState;
use design_domain::{Architecture, DesignUnit, Layer};
use evaluation_engine::EvaluationEngine;
use knowledge_engine::KnowledgeGraph;
use knowledge_lifecycle::LifecycleMetrics;
use language_core::semantic_parser;

#[test]
fn ai_context_is_deterministic_for_same_input() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    let architecture_state = ArchitectureState::from_architecture(&architecture, Vec::new());
    let semantic = semantic_parser("Build scalable REST API").semantic_graph;
    let evaluation = EvaluationEngine::default().evaluate(&architecture_state);

    let baseline = AIContext::new(AIContextParts {
        architecture_state: architecture_state.clone(),
        semantic_graph: semantic.clone(),
        knowledge_graph: KnowledgeGraph::default(),
        inferred_knowledge: Default::default(),
        stabilized_knowledge: Default::default(),
        lifecycle_metrics: LifecycleMetrics::default(),
        experience_state: ExperienceState::default(),
        evaluation_state: EvaluationState {
            latest: Some(evaluation),
            history: vec![evaluation],
        },
        runtime_state: RuntimeState {
            request_id: "req-1".to_string(),
            stage: "Output".to_string(),
            event_count: 3,
        },
    });

    let candidate = AIContext::new(AIContextParts {
        architecture_state,
        semantic_graph: semantic,
        knowledge_graph: KnowledgeGraph::default(),
        inferred_knowledge: Default::default(),
        stabilized_knowledge: Default::default(),
        lifecycle_metrics: LifecycleMetrics::default(),
        experience_state: ExperienceState::default(),
        evaluation_state: EvaluationState {
            latest: Some(evaluation),
            history: vec![evaluation],
        },
        runtime_state: RuntimeState {
            request_id: "req-1".to_string(),
            stage: "Output".to_string(),
            event_count: 3,
        },
    });

    assert_eq!(candidate, baseline);
}
