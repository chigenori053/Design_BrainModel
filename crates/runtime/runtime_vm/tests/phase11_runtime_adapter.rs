use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

#[test]
fn runtime_adapter_emits_simulation_events_and_summary() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("phase11 simulation".to_string());
    vm.execute();

    let phase = Phase9RuntimeAdapter::from_legacy(vm.context());
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();

    assert!(events.contains(&RuntimeEvent::SimulationStarted));
    assert!(events.contains(&RuntimeEvent::SimulationCompleted));
    assert!(phase
        .search_summary
        .as_ref()
        .map(|summary| summary.best_simulation_score > 0.0)
        .unwrap_or(false));
}
