use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

#[test]
fn knowledge_lifecycle_pipeline_persists_metrics_and_events() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("Build scalable REST API with service discovery".to_string());
    vm.execute();

    let phase = Phase9RuntimeAdapter::from_legacy(vm.context());
    let ai_context = phase.ai_context.as_ref().expect("ai context");
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();

    assert!(events.contains(&RuntimeEvent::KnowledgeProvenanceRecorded));
    assert!(events.contains(&RuntimeEvent::KnowledgeEmbeddingGenerated));
    assert!(events.contains(&RuntimeEvent::KnowledgeAgingApplied));
    assert!(events.contains(&RuntimeEvent::KnowledgeReinforced));
    assert!(events.contains(&RuntimeEvent::KnowledgeQualityAnalyzed));
    assert!(events.contains(&RuntimeEvent::KnowledgeSourceReliabilityEvaluated));
    assert!(events.contains(&RuntimeEvent::KnowledgeSemanticClustered));
    assert!(events.contains(&RuntimeEvent::KnowledgeEntropyCalculated));
    assert!(events.contains(&RuntimeEvent::KnowledgeHalfLifeCalculated));
    assert!(events.contains(&RuntimeEvent::LifecycleMetricsUpdated));
    assert!(events.contains(&RuntimeEvent::KnowledgeTurnoverAnalyzed));
    assert_eq!(ai_context.experience_state.graph.lifecycle_states.len(), 1);
    assert_eq!(ai_context.experience_state.graph.lifecycle_metrics.len(), 1);
    assert_eq!(ai_context.experience_state.graph.inferred_knowledges.len(), 1);
    assert_eq!(ai_context.experience_state.graph.stabilized_knowledges.len(), 1);
    assert!(ai_context.lifecycle_metrics.average_confidence > 0.0);
    assert!(ai_context.lifecycle_metrics.entropy > 0.0);
    assert!(ai_context.lifecycle_metrics.turnover_rate >= 0.0);
}
