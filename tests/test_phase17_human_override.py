import pytest
from fastapi.testclient import TestClient
import uuid

from design_brain_model.hybrid_vm.interface_layer.api_server import app
from design_brain_model.hybrid_vm.control_layer.state import HumanOverrideAction

client = TestClient(app)

@pytest.fixture
def initial_snapshot() -> dict:
    """Creates a snapshot via API."""
    resp = client.post("/snapshot/create")
    assert resp.status_code == 200
    return resp.json()["snapshot"]

@pytest.fixture
def snapshot_with_decision(initial_snapshot) -> tuple[dict, str]:
    """
    Creates a snapshot containing a DECISION semantic unit to be targeted.
    Returns the snapshot and the ID of the decision unit.
    """
    decision_id = str(uuid.uuid4())
    snapshot = initial_snapshot
    snapshot["vm_state"]["decision_state"]["decision_nodes"] = {
        decision_id: {
            "id": decision_id,
            "status": "REVIEW",
            "all_candidates": [{"candidate_id": "c1", "content": "Option A"}],
            "selected_candidate": {"candidate_id": "c1", "content": "Option A"},
            "confidence": "MID",
            "entropy": "MID",
            "human_override": False,
            "override_target_l2": None,
            "snapshot_before_override": None,
            "snapshot_after_override": None,
        }
    }
    return snapshot, decision_id


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
                "override_target_l2": "l2-fixed-1",
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
    decision_node = vm_state["decision_state"]["decision_nodes"][decision_id]
    assert decision_node["status"] == "OVERRIDDEN_L2"
    
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
                "override_target_l2": "l2-fixed-1",
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
                "override_target_l2": "l2-fixed-1",
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
                "override_target_l2": "l2-fixed-1",
                "override_action": "OVERRIDE_ACCEPT"
            }
        }
    }
    
    response = client.post("/event", json=event_payload)
    
    assert response.status_code == 400
    assert response.json()["error"] == "INVALID_OVERRIDE_PAYLOAD"
