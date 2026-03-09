use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

fn execute_phase9(input: &str) -> runtime_core::Phase9RuntimeContext {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text(input.to_string());
    vm.execute();
    Phase9RuntimeAdapter::from_legacy(vm.context())
}

#[test]
fn same_input_yields_same_event_sequence() {
    let left = execute_phase9("phase9 determinism");
    let right = execute_phase9("phase9 determinism");

    let left_events = left
        .event_bus
        .events()
        .cloned()
        .collect::<Vec<RuntimeEvent>>();
    let right_events = right
        .event_bus
        .events()
        .cloned()
        .collect::<Vec<RuntimeEvent>>();

    assert_eq!(left_events, right_events);
}

#[test]
fn same_input_yields_same_hypothesis() {
    let left = execute_phase9("phase9 determinism");
    let right = execute_phase9("phase9 determinism");

    assert_eq!(left.hypotheses, right.hypotheses);
}

#[test]
fn same_input_yields_same_recall_candidates() {
    let left = execute_phase9("phase9 determinism");
    let right = execute_phase9("phase9 determinism");

    assert_eq!(left.recall_result, right.recall_result);
}

#[test]
fn same_input_yields_same_consistency() {
    let left = execute_phase9("phase9 determinism");
    let right = execute_phase9("phase9 determinism");

    assert_eq!(left.evaluation, right.evaluation);
}

#[test]
fn adapter_lift_is_stable() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("phase9 determinism".to_string());
    vm.execute();

    let first = Phase9RuntimeAdapter::snapshot(vm.context());
    let second = Phase9RuntimeAdapter::snapshot(vm.context());

    assert_eq!(first.request_id, second.request_id);
    assert_eq!(first.modality, second.modality);
    assert_eq!(first.stage, second.stage);
    assert_eq!(first.recalled_memories, second.recalled_memories);
    assert_eq!(first.hypotheses, second.hypotheses);
    assert_eq!(first.events, second.events);
}
