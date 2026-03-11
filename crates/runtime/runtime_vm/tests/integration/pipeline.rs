use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

#[test]
fn pipeline_execution_and_context_propagation() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("optimize database query with cache");
    vm.execute();

    let ctx = vm.context();
    assert!(!ctx.semantic_units.is_empty());
    assert!(!ctx.concepts.is_empty());
    assert!(!ctx.intent_nodes.is_empty());
    assert!(ctx.search_state.is_some());
    assert!(ctx.hypothesis_graph.is_some());
    assert!(ctx.design_state.is_some());
}

#[test]
fn pipeline_phase17_initializes_ai_context_and_updates_experience_graph() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("Build scalable REST API".to_string());
    vm.execute();

    let phase = Phase9RuntimeAdapter::from_legacy(vm.context());
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();
    let ai_context = phase.ai_context.as_ref().expect("ai context");

    assert!(events.contains(&RuntimeEvent::AIContextInitialized));
    assert!(events.contains(&RuntimeEvent::KnowledgeQueryIssued));
    assert!(events.contains(&RuntimeEvent::KnowledgeRetrieved));
    assert!(events.contains(&RuntimeEvent::KnowledgeParsed));
    assert!(events.contains(&RuntimeEvent::KnowledgeValidated));
    assert!(events.contains(&RuntimeEvent::KnowledgeEffectiveConfidenceCalculated));
    assert!(events.contains(&RuntimeEvent::KnowledgeIntegrated));
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
    assert!(events.contains(&RuntimeEvent::KnowledgeConflictResolvedWithContext)
        || events.contains(&RuntimeEvent::KnowledgeConflictResolved)
        || !ai_context.knowledge_graph.relations.is_empty());
    assert!(events.contains(&RuntimeEvent::ArchitectureStateCreated));
    assert!(events.contains(&RuntimeEvent::EvaluationStarted));
    assert!(events.contains(&RuntimeEvent::EvaluationCompleted));
    assert!(events.contains(&RuntimeEvent::ExperienceGraphUpdated));
    assert!(!ai_context.architecture_state.components.is_empty());
    assert!(!ai_context.knowledge_graph.entities.is_empty());
    assert!(!ai_context.inferred_knowledge.graph.entities.is_empty());
    assert!(!ai_context.stabilized_knowledge.graph.entities.is_empty());
    assert!(ai_context.lifecycle_metrics.average_confidence > 0.0);
    assert!(ai_context.lifecycle_metrics.entropy > 0.0);
    assert!(ai_context.lifecycle_metrics.turnover_rate >= 0.0);
    assert!(ai_context.lifecycle_metrics.half_life <= vm.context().tick as u64 || vm.context().tick == 0);
    assert_eq!(ai_context.experience_state.graph.edges.len(), 1);
    assert_eq!(ai_context.experience_state.graph.knowledges.len(), 1);
    assert_eq!(ai_context.experience_state.graph.lifecycle_states.len(), 1);
    assert_eq!(ai_context.experience_state.graph.lifecycle_metrics.len(), 1);
    assert!(ai_context.evaluation_state.latest.is_some());
}
