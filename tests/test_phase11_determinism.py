from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.events import UserInputEvent
from design_brain_model.hybrid_vm.control_layer.state import DecisionCandidate, Policy, Role


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


def test_determinism_same_input_repeat():
    policy = Policy(
        name="Performance First",
        weights={"performance": 0.8, "cost": 0.1, "risk": 0.1, "maintainability": 0.0, "scalability": 0.0},
    )
    signatures = []

    for _ in range(10):
        vm = HybridVM.create()
        candidates = _build_candidates("q1")
        vm.evaluate_decision("q1", candidates, policy)
        snapshot = vm.get_state_snapshot()
        outcome_obj = snapshot["decision_state"]["outcomes"][-1]

        signature = {
            "consensus_status": outcome_obj.get("consensus_status"),
            "ranked_candidates": [
                {
                    "content": c["content"],
                    "final_score": c["final_score"],
                    "utility": c["utility_vector_snapshot"],
                }
                for c in outcome_obj["ranked_candidates"]
            ],
            "explanation": outcome_obj["explanation"],
        }
        signatures.append(signature)

    assert all(sig == signatures[0] for sig in signatures)


def test_parallel_vm_isolation():
    vms = [HybridVM.create() for _ in range(10)]
    for vm in vms:
        vm.process_event(UserInputEvent(payload={"content": "use database"}))

    for vm in vms:
        snapshot = vm.get_state_snapshot()
        history = snapshot["conversation"]["history"]
        units = snapshot["semantic_units"]["units"]
        assert len(history) == 1
        assert len(units) >= 1


def test_snapshot_replay_determinism():
    policy = Policy(
        name="Cost Saver",
        weights={"performance": 0.1, "cost": 0.8, "risk": 0.1, "maintainability": 0.0, "scalability": 0.0},
    )

    vm = HybridVM.create()
    candidates = _build_candidates("q2")
    vm.evaluate_decision("q2", candidates, policy)
    snapshot = vm.get_state_snapshot()

    first = snapshot["decision_state"]["outcomes"][-1]
    first_sig = {
        "consensus_status": first.get("consensus_status"),
        "ranked_candidates": [
            {
                "content": c["content"],
                "final_score": c["final_score"],
                "utility": c["utility_vector_snapshot"],
            }
            for c in first["ranked_candidates"]
        ],
        "explanation": first["explanation"],
    }

    vm_replay = HybridVM.from_snapshot(snapshot, vm_id=vm.vm_id)
    candidates_replay = _build_candidates("q2")
    vm_replay.evaluate_decision("q2", candidates_replay, policy)
    snapshot_replay = vm_replay.get_state_snapshot()
    second = snapshot_replay["decision_state"]["outcomes"][-1]

    second_sig = {
        "consensus_status": second.get("consensus_status"),
        "ranked_candidates": [
            {
                "content": c["content"],
                "final_score": c["final_score"],
                "utility": c["utility_vector_snapshot"],
            }
            for c in second["ranked_candidates"]
        ],
        "explanation": second["explanation"],
    }

    assert first_sig == second_sig
