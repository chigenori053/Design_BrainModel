
import pytest
from unittest.mock import MagicMock, patch
from design_brain_model.hybrid_vm.interface_layer.api_server import CommandRequest
from design_brain_model.hybrid_vm.core import HybridVM, HumanOverrideError, UserInputError
from design_brain_model.hybrid_vm.control_layer.state import SemanticUnit, SemanticUnitKind, SemanticUnitStatus, HumanOverrideAction
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

    def test_human_override_empty_resolves(self):
        """Verify HumanOverrideError is raised when target node has no resolves."""
        vm = HybridVM()
        target_id = "decision_1"
        
        # Mock State
        mock_unit = MagicMock(spec=SemanticUnit)
        mock_unit.id = target_id
        mock_unit.kind = SemanticUnitKind.DECISION
        mock_unit.resolves = [] # Empty
        vm._state.semantic_units.units[target_id] = mock_unit
        
        payload = {
            "target_decision_id": target_id,
            "override_action": "OVERRIDE_ACCEPT",
            "reason": "Test"
        }
        
        with pytest.raises(HumanOverrideError, match="No override target available"):
            vm._handle_human_override(payload, "event_1")

    def test_human_override_multiple_resolves(self):
        """Verify HumanOverrideError is raised when target node has multiple resolves."""
        vm = HybridVM()
        target_id = "decision_1"
        
        # Mock State
        mock_unit = MagicMock(spec=SemanticUnit)
        mock_unit.id = target_id
        mock_unit.kind = SemanticUnitKind.DECISION
        mock_unit.resolves = ["q1", "q2"] # Multiple
        vm._state.semantic_units.units[target_id] = mock_unit
        
        payload = {
            "target_decision_id": target_id,
            "override_action": "OVERRIDE_ACCEPT",
            "reason": "Test"
        }
        
        with pytest.raises(HumanOverrideError, match="Ambiguous override target"):
            vm._handle_human_override(payload, "event_1")

    def test_human_override_single_resolves_success(self):
        """Verify success when target node has exactly one resolve."""
        vm = HybridVM()
        target_id = "decision_1"
        
        # Mock State
        mock_unit = SemanticUnit(
            id=target_id,
            kind=SemanticUnitKind.DECISION,
            content="test",
            status=SemanticUnitStatus.UNSTABLE,
            resolves=["q1"], # Single
            confidence=1.0,
            origin_event_id="e1",
            source_message_id="m1"
        )
        vm._state.semantic_units.units[target_id] = mock_unit
        
        payload = {
            "target_decision_id": target_id,
            "override_action": HumanOverrideAction.ACCEPT,
            "reason": "Test"
        }
        
        # Should not raise
        vm._handle_human_override(payload, "event_1")
        assert mock_unit.status == SemanticUnitStatus.STABLE

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

