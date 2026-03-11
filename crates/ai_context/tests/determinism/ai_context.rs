use ai_context::{AIContext, EvaluationState, ExperienceState, RuntimeState};
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

    let baseline = AIContext::new(
        architecture_state.clone(),
        semantic.clone(),
        KnowledgeGraph::default(),
        Default::default(),
        Default::default(),
        LifecycleMetrics::default(),
        ExperienceState::default(),
        EvaluationState {
            latest: Some(evaluation),
            history: vec![evaluation],
        },
        RuntimeState {
            request_id: "req-1".to_string(),
            stage: "Output".to_string(),
            event_count: 3,
        },
    );

    let candidate = AIContext::new(
        architecture_state,
        semantic,
        KnowledgeGraph::default(),
        Default::default(),
        Default::default(),
        LifecycleMetrics::default(),
        ExperienceState::default(),
        EvaluationState {
            latest: Some(evaluation),
            history: vec![evaluation],
        },
        RuntimeState {
            request_id: "req-1".to_string(),
            stage: "Output".to_string(),
            event_count: 3,
        },
    );

    assert_eq!(candidate, baseline);
}
