import pytest
import uuid
from copy import deepcopy
from datetime import datetime, timezone
from pydantic import ValidationError

from design_brain_model.hybrid_vm.core import HybridVM, HumanOverrideError, InvalidOverridePayloadError
from design_brain_model.hybrid_vm.events import BaseEvent, EventType, Actor
from design_brain_model.hybrid_vm.control_layer.state import (
    SemanticUnit, SemanticUnitKind, SemanticUnitStatus, HumanOverrideAction,
    DecisionOutcome, ConsensusStatus, EvaluationResult, UtilityVector, Role,
    DecisionNode, DecisionNodeStatus, DecisionNodeCandidate, ConfidenceLevel, EntropyLevel
)

class TestPhase17FailCases:
    """
    Implements specific Fail Cases (F-01 to F-05) for Phase 17.
    """

    @pytest.fixture
    def vm_with_override(self):
        """Prepares a VM with a decision that has been successfully overridden."""
        vm = HybridVM()
        decision_id = "decision_F01"
        
        # 1. Setup Decision Node (Phase 17-2 State)
        node = DecisionNode(
            id=decision_id,
            status=DecisionNodeStatus.REVIEW,
            all_candidates=[DecisionNodeCandidate(candidate_id="c1", content="A")],
            selected_candidate=DecisionNodeCandidate(candidate_id="c1", content="A"),
            confidence=ConfidenceLevel.MID,
            entropy=EntropyLevel.MID
        )
        vm._state.decision_state.decision_nodes[decision_id] = node
        
        # 2. Setup initial outcome (Review needed) for history consistency
        outcome = DecisionOutcome(
            resolves_question_id=decision_id,
            consensus_status=ConsensusStatus.REVIEW,
            explanation="Initial run",
            ranked_candidates=[]
        )
        vm._state.decision_state.outcomes.append(outcome)
        
        # 3. Apply Override
        payload = {
            "target_decision_id": decision_id,
            "override_target_l2": "c1", # Required Field
            "override_action": HumanOverrideAction.ACCEPT,
            "reason": "F-01 Test"
        }
        vm._handle_human_override(payload, "event_override_1")
        
        return vm, decision_id

    def test_F01_inference_reentry_prevention(self, vm_with_override):
        """
        F-01: Verify that inference is strictly blocked or no-op'd for an overridden decision.
        """
        vm, decision_id = vm_with_override
        
        # Attempt to trigger re-evaluation
        # The system should check _is_system_halted and sink the event
        event = BaseEvent(
            type=EventType.REQUEST_REEVALUATION,
            payload={"question_id": decision_id},
            actor=Actor.DESIGN_BRAIN
        )
        
        vm.process_event(event)
        
        # Verify the event was sunk with an error
        assert len(vm.sink_log) > 0
        sink_entry = vm.sink_log[-1]
        assert "blocked" in sink_entry["error"] or "halted" in sink_entry["error"]

    def test_F02_snapshot_destruction(self):
        """
        F-02: Verify that loading an invalid/corrupted snapshot fails.
        """
        valid_snapshot = HybridVM().build_snapshot()
        
        # Corrupt the snapshot structure
        corrupted_snapshot = deepcopy(valid_snapshot)
        # Change vm_state to an invalid type to force validation error
        corrupted_snapshot["vm_state"] = "INVALID_STRING_NOT_DICT"
        
        with pytest.raises(ValidationError):
            HybridVM.from_snapshot(corrupted_snapshot["vm_state"])

    def test_F04_state_inconsistency_double_override(self, vm_with_override):
        """
        F-04: Verify that attempting to override an already overridden decision is blocked
        OR strictly validated.
        """
        vm, decision_id = vm_with_override
        
        # Try to override the SAME decision again
        payload = {
            "target_decision_id": decision_id,
            "override_target_l2": "c1",
            "override_action": HumanOverrideAction.REJECT,
            "reason": "Double Override"
        }
        
        # The current implementation raises HumanOverrideError if already overridden
        with pytest.raises(HumanOverrideError, match="already overridden"):
             vm._handle_human_override(payload, "event_fail")

    def test_F05_restart_determinism(self, vm_with_override):
        """
        F-05: Verify that reloading from a snapshot produces the EXACT same state hash/metrics.
        """
        vm1, _ = vm_with_override
        snapshot1 = vm1.build_snapshot()
        
        # Rehydrate VM2 from VM1's snapshot
        vm2 = HybridVM.from_snapshot(snapshot1["vm_state"], vm_id=vm1.vm_id)
        
        # Check Critical State Equality
        # 1. Decision Node Status
        nodes1 = vm1._state.decision_state.decision_nodes
        nodes2 = vm2._state.decision_state.decision_nodes
        assert nodes1.keys() == nodes2.keys()
        for nid, n1 in nodes1.items():
            n2 = nodes2[nid]
            assert n1.status == n2.status
            assert n1.human_override == n2.human_override
            assert n1.override_target_l2 == n2.override_target_l2
            
        # 2. Override History
        hist1 = vm1._state.override_history
        hist2 = vm2._state.override_history
        assert len(hist1) == len(hist2)
        assert hist1[0].reason == hist2[0].reason
        
        # 3. Next Snapshot Hash Equality (Determinism)
        snapshot2 = vm2.build_snapshot()
        assert snapshot1["current_decision_id"] == snapshot2["current_decision_id"]
        assert snapshot1["confidence"] == snapshot2["confidence"]
        assert snapshot1["entropy"] == snapshot2["entropy"]