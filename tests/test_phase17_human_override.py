import pytest
from fastapi.testclient import TestClient
import uuid

from design_brain_model.hybrid_vm.interface_layer.api_server import app
from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.control_layer.state import SemanticUnit, SemanticUnitKind, SemanticUnitStatus, HumanOverrideAction

client = TestClient(app)

@pytest.fixture
def initial_snapshot() -> dict:
    """Creates a VM and returns its initial snapshot."""
    vm = HybridVM.create()
    return vm.build_snapshot()

@pytest.fixture
def snapshot_with_decision(initial_snapshot) -> tuple[dict, str]:
    """
    Creates a snapshot containing a DECISION semantic unit to be targeted.
    Returns the snapshot and the ID of the decision unit.
    """
    vm = HybridVM.from_snapshot(initial_snapshot["vm_state"])
    
    # Create a Question and a Decision unit that resolves it
    question_id = str(uuid.uuid4())
    decision_id = str(uuid.uuid4())

    question_unit = SemanticUnit(
        id=question_id,
        kind=SemanticUnitKind.QUESTION,
        content="Which framework to use?",
        status=SemanticUnitStatus.STABLE
    )
    decision_unit = SemanticUnit(
        id=decision_id,
        kind=SemanticUnitKind.DECISION,
        content="Decision about framework.",
        status=SemanticUnitStatus.REVIEW, # Initial state to be overridden
        resolves={question_id}
    )

    vm._state.semantic_units.units[question_id] = question_unit
    vm._state.semantic_units.units[decision_id] = decision_unit
    
    return vm.build_snapshot(), decision_id


def test_override_success(snapshot_with_decision):
    """
    Tests a successful HUMAN_OVERRIDE flow.
    """
    snapshot, decision_id = snapshot_with_decision
    
    event_payload = {
        "snapshot": snapshot,
        "payload": {
            "action": "HUMAN_OVERRIDE",
            "data": {
                "target_decision_id": decision_id,
                "override_action": "OVERRIDE_ACCEPT",
                "reason": "Team consensus."
            }
        }
    }
    
    response = client.post("/event", json=event_payload)
    
    assert response.status_code == 200
    new_snapshot = response.json()["snapshot"]
    
    # Verify the snapshot was updated
    assert new_snapshot["snapshot_id"] != snapshot["snapshot_id"]
    
    # Verify the state change in the new snapshot
    vm_state = new_snapshot["vm_state"]
    overridden_unit = vm_state["semantic_units"]["units"][decision_id]
    assert overridden_unit["status"] == "stable" # Corresponds to SemanticUnitStatus.STABLE
    
    # Verify the override history
    override_history = vm_state["override_history"]
    assert len(override_history) == 1
    record = override_history[0]
    assert record["decision_id"] == decision_id
    assert record["override_status"] == "OVERRIDE_ACCEPT"
    assert record["reason"] == "Team consensus."
    assert record["overridden_by"] == "HUMAN"


def test_override_decision_not_found(initial_snapshot):
    """
    Tests that a 404 is returned if the target_decision_id does not exist.
    """
    non_existent_id = str(uuid.uuid4())
    event_payload = {
        "snapshot": initial_snapshot,
        "payload": {
            "action": "HUMAN_OVERRIDE",
            "data": {
                "target_decision_id": non_existent_id,
                "override_action": "OVERRIDE_ACCEPT"
            }
        }
    }
    
    response = client.post("/event", json=event_payload)
    
    assert response.status_code == 404
    assert response.json()["error"] == "DECISION_NOT_FOUND"

def test_override_invalid_payload_action(snapshot_with_decision):
    """
    Tests that a 400 is returned for an invalid override_action.
    """
    snapshot, decision_id = snapshot_with_decision
    
    event_payload = {
        "snapshot": snapshot,
        "payload": {
            "action": "HUMAN_OVERRIDE",
            "data": {
                "target_decision_id": decision_id,
                "override_action": "INVALID_ACTION_VALUE" 
            }
        }
    }
    
    response = client.post("/event", json=event_payload)
    
    assert response.status_code == 400
    assert response.json()["error"] == "INVALID_OVERRIDE_PAYLOAD"


def test_override_invalid_payload_missing_id(snapshot_with_decision):
    """
    Tests that a 400 is returned if target_decision_id is missing.
    """
    snapshot, _ = snapshot_with_decision
    
    event_payload = {
        "snapshot": snapshot,
        "payload": {
            "action": "HUMAN_OVERRIDE",
            "data": {
                # target_decision_id is missing
                "override_action": "OVERRIDE_ACCEPT"
            }
        }
    }
    
    response = client.post("/event", json=event_payload)
    
    assert response.status_code == 400
    assert response.json()["error"] == "INVALID_OVERRIDE_PAYLOAD"
