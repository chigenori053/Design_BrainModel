import uuid

from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.events import BaseEvent, EventType
from design_brain_model.hybrid_vm.control_layer.state import DecisionNode, DecisionNodeStatus


def test_event_coverage_and_termination():
    vm = HybridVM.create()

    # Create a dummy decision node for the HUMAN_OVERRIDE event to target
    vm._state.decision_state.decision_nodes["d1"] = DecisionNode(
        id="d1",
        status=DecisionNodeStatus.REVIEW,
    )

    for event_type in EventType:
        payload = {}
        if event_type == EventType.USER_INPUT:
            payload = {"content": "test"}
        if event_type == EventType.HUMAN_OVERRIDE:
            payload = {
                "override_action": "OVERRIDE_ACCEPT",
                "reason": "test",
                "target_decision_id": "d1",
                "override_target_l2": "l2-1",
            }
        ev = BaseEvent(type=event_type, payload=payload)
        vm.process_event(ev)

    # Each event type should appear at least once in the log
    seen = {e.type for e in vm.event_log}
    assert set(EventType).issubset(seen)


def test_event_lineage_metadata():
    vm = HybridVM.create()
    first = BaseEvent(type=EventType.USER_INPUT, payload={"content": "a"})
    second = BaseEvent(type=EventType.EXECUTION_REQUEST, payload={})

    vm.process_event(first)
    vm.process_event(second)

    first_event = vm.event_log[0]
    second_event = vm.event_log[1]

    # UUID format
    uuid.UUID(first_event.event_id)
    uuid.UUID(second_event.event_id)

    # lineage and metadata
    assert second_event.parent_event_id == first_event.event_id
    assert first_event.vm_id == vm.vm_id
    assert second_event.vm_id == vm.vm_id
    assert first_event.logical_index == 1
    assert second_event.logical_index == 2
    assert first_event.wall_timestamp is not None
    assert second_event.wall_timestamp is not None
