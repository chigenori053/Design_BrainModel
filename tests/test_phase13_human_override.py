from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.control_layer.state import DecisionCandidate, Policy, Role, ConsensusStatus
from design_brain_model.hybrid_vm.events import RequestReevaluationEvent, EventType


def _build_candidates(question_id: str):
    return [
        DecisionCandidate(
            resolves_question_id=question_id,
            content="Use Hash Sharding (High Perf, Complex)",
            proposed_by=Role.BRAIN,
        ),
        DecisionCandidate(
            resolves_question_id=question_id,
            content="Use Range Sharding (Easy, Scale risk)",
            proposed_by=Role.USER,
        ),
    ]


def test_override_priority_over_consensus():
    vm = HybridVM.create()
    policy = Policy(name="Performance First", weights={"performance": 1.0})
    vm.evaluate_decision("q1", _build_candidates("q1"), policy)
    snapshot = vm.get_state_snapshot()
    last_outcome_id = snapshot["decision_state"]["outcomes"][-1]["outcome_id"]

    outcome = vm.process_human_override(
        override_action="REJECT",
        reason="Human rejects",
        target_decision_id=last_outcome_id,
        candidate_ids=[],
    )

    assert outcome.consensus_status == ConsensusStatus.REJECT
    assert outcome.overridden_decision_id == last_outcome_id
    assert outcome.override_event_id is None or isinstance(outcome.override_event_id, str)


def test_override_blocks_reevaluation():
    vm = HybridVM.create()
    policy = Policy(name="Cost Saver", weights={"cost": 1.0})
    vm.evaluate_decision("q2", _build_candidates("q2"), policy)
    snapshot = vm.get_state_snapshot()
    last_outcome_id = snapshot["decision_state"]["outcomes"][-1]["outcome_id"]

    vm.process_human_override(
        override_action="ACCEPT",
        reason="Human approves",
        target_decision_id=last_outcome_id,
        candidate_ids=[],
    )

    vm.process_event(RequestReevaluationEvent(type=EventType.REQUEST_REEVALUATION, payload={}))
    assert any(entry.get("error") == "Reevaluation blocked after human override" for entry in vm.sink_log)


def test_override_determinism():
    vm = HybridVM.create()
    outcome1 = vm.process_human_override(
        override_action="FORCE_REVIEW",
        reason="Need review",
        target_decision_id="decision-123",
        candidate_ids=[],
    )
    vm2 = HybridVM.create()
    outcome2 = vm2.process_human_override(
        override_action="FORCE_REVIEW",
        reason="Need review",
        target_decision_id="decision-123",
        candidate_ids=[],
    )
    assert outcome1.outcome_id == outcome2.outcome_id
