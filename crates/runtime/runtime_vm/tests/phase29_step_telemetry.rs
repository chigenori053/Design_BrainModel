use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

fn execute_phase29(input: &str) -> Vec<RuntimeEvent> {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text(input.to_string());
    vm.execute();
    Phase9RuntimeAdapter::from_legacy(vm.context())
        .event_bus
        .events()
        .cloned()
        .collect()
}

fn event_index(events: &[RuntimeEvent], target: RuntimeEvent) -> usize {
    events
        .iter()
        .position(|event| *event == target)
        .expect("event must exist")
}

#[test]
fn phase29_emits_simulation_step_events() {
    let events = execute_phase29("phase29 simulation step telemetry");
    let step_count = events
        .iter()
        .filter(|event| **event == RuntimeEvent::SimulationStep)
        .count();

    assert!(events.contains(&RuntimeEvent::SimulationStarted));
    assert!(events.contains(&RuntimeEvent::SimulationCompleted));
    assert!(step_count > 0);
}

#[test]
fn phase29_simulation_step_events_are_ordered_between_start_and_completion() {
    let events = execute_phase29("phase29 simulation step ordering");
    let started = event_index(&events, RuntimeEvent::SimulationStarted);
    let completed = event_index(&events, RuntimeEvent::SimulationCompleted);
    let step_indices = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (*event == RuntimeEvent::SimulationStep).then_some(index))
        .collect::<Vec<_>>();

    assert!(!step_indices.is_empty());
    assert!(step_indices.iter().all(|index| started < *index && *index < completed));
}

#[test]
fn phase29_simulation_step_telemetry_is_deterministic() {
    let left = execute_phase29("phase29 deterministic simulation step telemetry");
    let right = execute_phase29("phase29 deterministic simulation step telemetry");

    assert_eq!(left, right);
}
