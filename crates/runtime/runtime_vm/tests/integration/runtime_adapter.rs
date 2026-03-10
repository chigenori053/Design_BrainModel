use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

#[test]
fn runtime_adapter_emits_simulation_events_and_summary() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("phase11 simulation".to_string());
    vm.execute();

    let phase = Phase9RuntimeAdapter::from_legacy(vm.context());
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();

    assert!(events.contains(&RuntimeEvent::LanguageParsingStarted));
    assert!(events.contains(&RuntimeEvent::LanguageParsingCompleted));
    assert!(events.contains(&RuntimeEvent::MeaningReasoningStarted));
    assert!(events.contains(&RuntimeEvent::SemanticInferenceApplied));
    assert!(events.contains(&RuntimeEvent::MeaningReasoningCompleted));
    assert!(events.contains(&RuntimeEvent::LanguageSearchStarted));
    assert!(events.contains(&RuntimeEvent::LanguageSearchCompleted));
    assert!(events.contains(&RuntimeEvent::SimulationStarted));
    assert!(events.contains(&RuntimeEvent::SimulationCompleted));
    assert!(events.contains(&RuntimeEvent::PatternMatchStarted));
    assert!(events.contains(&RuntimeEvent::PatternMatchCompleted));
    assert!(events.contains(&RuntimeEvent::PolicyEvaluationStarted));
    assert!(events.contains(&RuntimeEvent::PolicyEvaluationCompleted));
    assert!(events.contains(&RuntimeEvent::CausalAnalysisStarted));
    assert!(events.contains(&RuntimeEvent::CausalClosureComputed));
    assert!(events.contains(&RuntimeEvent::CausalValidationPassed));
    assert!(events.contains(&RuntimeEvent::ExperienceStored));
    assert!(events.contains(&RuntimeEvent::PolicyUpdated));
    assert!(
        phase
            .search_summary
            .as_ref()
            .map(|summary| summary.best_simulation_score > 0.0)
            .unwrap_or(false)
    );
}
