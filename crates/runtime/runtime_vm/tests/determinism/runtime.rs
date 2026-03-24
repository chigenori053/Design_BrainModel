use runtime_core::RuntimeEvent;
use runtime_vm::{ExecutionMode, Phase9RuntimeAdapter, dbm_test, test_support::with_test_vm};

fn execute_phase9(
    runtime: &mut runtime_vm::RuntimeContext,
    input: &str,
) -> runtime_core::Phase9RuntimeContext {
    with_test_vm(runtime, ExecutionMode::Reasoning, |vm| {
        vm.set_input_text(input.to_string());
        vm.execute();
        Phase9RuntimeAdapter::from_legacy(vm.context())
    })
}

dbm_test!(same_input_yields_same_event_sequence, runtime, {
    let left = execute_phase9(runtime, "phase9 determinism");
    let right = execute_phase9(runtime, "phase9 determinism");

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
});

dbm_test!(same_input_yields_same_hypothesis, runtime, {
    let left = execute_phase9(runtime, "phase9 determinism");
    let right = execute_phase9(runtime, "phase9 determinism");

    assert_eq!(left.hypotheses, right.hypotheses);
});

dbm_test!(same_input_yields_same_recall_candidates, runtime, {
    let left = execute_phase9(runtime, "phase9 determinism");
    let right = execute_phase9(runtime, "phase9 determinism");

    assert_eq!(left.recall_result, right.recall_result);
});

dbm_test!(same_input_yields_same_consistency, runtime, {
    let left = execute_phase9(runtime, "phase9 determinism");
    let right = execute_phase9(runtime, "phase9 determinism");

    assert_eq!(left.evaluation, right.evaluation);
});

dbm_test!(adapter_lift_is_stable, runtime, {
    with_test_vm(runtime, ExecutionMode::Reasoning, |vm| {
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
    });
});
