use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

#[test]
fn pipeline_phase17_initializes_ai_context_and_updates_experience_graph() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("Build scalable REST API".to_string());
    vm.execute();

    let phase = Phase9RuntimeAdapter::from_legacy(vm.context());
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();
    let ai_context = phase.ai_context.as_ref().expect("ai context");

    assert!(events.contains(&RuntimeEvent::AIContextInitialized));
    assert!(events.contains(&RuntimeEvent::ArchitectureStateCreated));
    assert!(events.contains(&RuntimeEvent::EvaluationStarted));
    assert!(events.contains(&RuntimeEvent::EvaluationCompleted));
    assert!(events.contains(&RuntimeEvent::ExperienceGraphUpdated));
    assert!(!ai_context.architecture_state.components.is_empty());
    assert_eq!(ai_context.experience_state.graph.edges.len(), 1);
    assert!(ai_context.evaluation_state.latest.is_some());
}
