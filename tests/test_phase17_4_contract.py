import pytest
from unittest.mock import MagicMock
from design_brain_model.hybrid_vm.interface_layer.api_server import CommandRequest
from design_brain_model.hybrid_vm.core import HybridVM, HumanOverrideError, UserInputError, InvalidOverridePayloadError
from design_brain_model.hybrid_vm.control_layer.state import (
    SemanticUnit, SemanticUnitKind, SemanticUnitStatus, HumanOverrideAction,
    DecisionNode, DecisionNodeStatus, DecisionNodeCandidate, ConfidenceLevel, EntropyLevel
)
from design_brain_model.hybrid_vm.events import BaseEvent, EventType, Actor

class TestPhase17_4_Contract:
    
    def test_api_command_request_alias(self):
        """Verify CommandRequest accepts commandType (camelCase)."""
        payload = {"some": "data"}
        
        # Test camelCase
        req_camel = CommandRequest(**{"commandType": "CreateL1Atom", "payload": payload})
        assert req_camel.command_type == "CreateL1Atom"
        
        # Test snake_case (standard)
        req_snake = CommandRequest(command_type="CreateL1Atom", payload=payload)
        assert req_snake.command_type == "CreateL1Atom"

    def test_human_override_missing_target_l2(self):
        """Verify HumanOverrideError when override_target_l2 is missing."""
        vm = HybridVM()
        target_id = "decision_1"
        vm._state.decision_state.decision_nodes[target_id] = DecisionNode(
            id=target_id,
            status=DecisionNodeStatus.REVIEW,
            all_candidates=[DecisionNodeCandidate(candidate_id="c1", content="A")],
            selected_candidate=DecisionNodeCandidate(candidate_id="c1", content="A"),
            confidence=ConfidenceLevel.MID,
            entropy=EntropyLevel.MID,
        )
        payload = {
            "target_decision_id": target_id,
            "override_action": "OVERRIDE_ACCEPT",
            "reason": "Test"
            # override_target_l2 missing
        }
        
        # Pydantic validation error expected via InvalidOverridePayloadError
        with pytest.raises(InvalidOverridePayloadError):
            vm._handle_human_override(payload, "event_1")

    def test_human_override_success(self):
        """Verify success with valid payload."""
        vm = HybridVM()
        target_id = "decision_1"
        
        vm._state.decision_state.decision_nodes[target_id] = DecisionNode(
            id=target_id,
            status=DecisionNodeStatus.REVIEW,
            all_candidates=[DecisionNodeCandidate(candidate_id="c1", content="A")],
            selected_candidate=DecisionNodeCandidate(candidate_id="c1", content="A"),
            confidence=ConfidenceLevel.MID,
            entropy=EntropyLevel.MID,
        )
        
        payload = {
            "target_decision_id": target_id,
            "override_target_l2": "c1",
            "override_action": HumanOverrideAction.ACCEPT,
            "reason": "Test"
        }
        
        # Should not raise
        vm._handle_human_override(payload, "event_1")
        assert vm._state.decision_state.decision_nodes[target_id].status == DecisionNodeStatus.OVERRIDDEN_L2

    def test_user_input_validation(self):
        """Verify UserInputError for invalid content."""
        vm = HybridVM()
        
        # 1. None content
        event_none = BaseEvent(
            type=EventType.USER_INPUT,
            payload={"content": None},
            actor=Actor.USER,
            event_id="e1",
            vm_id="vm1",
            logical_index=1
        )
        with pytest.raises(UserInputError, match="Empty input"):
            vm._handle_user_input(event_none)

        # 2. Empty String
        event_empty = BaseEvent(
            type=EventType.USER_INPUT,
            payload={"content": ""},
            actor=Actor.USER,
            event_id="e2",
            vm_id="vm1",
            logical_index=2
        )
        with pytest.raises(UserInputError, match="Empty input"):
            vm._handle_user_input(event_empty)
            
        # 3. Whitespace String
        event_ws = BaseEvent(
            type=EventType.USER_INPUT,
            payload={"content": "   "},
            actor=Actor.USER,
            event_id="e3",
            vm_id="vm1",
            logical_index=3
        )
        with pytest.raises(UserInputError, match="Empty input"):
            vm._handle_user_input(event_ws)