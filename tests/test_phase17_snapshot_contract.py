import pytest
fastapi = pytest.importorskip("fastapi")
from fastapi.testclient import TestClient

from design_brain_model.hybrid_vm.interface_layer.api_server import app


client = TestClient(app)


def _create_snapshot():
    resp = client.post("/snapshot/create")
    assert resp.status_code == 200
    body = resp.json()
    assert "snapshot" in body
    snapshot = body["snapshot"]
    assert "snapshot_id" in snapshot
    assert "vm_state" in snapshot
    return snapshot


def test_snapshot_required_for_latest_and_history():
    resp = client.request("GET", "/decision/latest")
    assert resp.status_code == 400
    assert resp.json() == {"error": "SNAPSHOT_REQUIRED"}

    resp = client.request("GET", "/decision/history")
    assert resp.status_code == 400
    assert resp.json() == {"error": "SNAPSHOT_REQUIRED"}


def test_snapshot_required_for_event():
    resp = client.post("/event", json={"payload": {"action": "USER_INPUT", "data": {"content": "hi"}}})
    assert resp.status_code == 400
    assert resp.json() == {"error": "SNAPSHOT_REQUIRED"}


def test_snapshot_mismatch_rejected():
    resp = client.request("GET", "/decision/latest", json={"snapshot": {"snapshot_id": "x"}})
    assert resp.status_code == 409
    assert resp.json() == {"error": "SNAPSHOT_MISMATCH"}

    resp = client.post("/event", json={"snapshot": {"snapshot_id": "x"}, "payload": {"action": "USER_INPUT"}})
    assert resp.status_code == 409
    assert resp.json() == {"error": "SNAPSHOT_MISMATCH"}


def test_event_updates_snapshot_state():
    snapshot = _create_snapshot()

    resp = client.post(
        "/event",
        json={
            "snapshot": snapshot,
            "payload": {
                "action": "USER_INPUT",
                "data": {"content": "hello"},
            },
        },
    )
    assert resp.status_code == 200
    body = resp.json()
    assert "snapshot" in body
    updated = body["snapshot"]
    assert updated.get("vm_state", {}).get("conversation", {}).get("history")
    assert updated["vm_state"]["conversation"]["history"][0]["content"] == "hello"


def test_latest_uses_snapshot_body():
    snapshot = _create_snapshot()
    resp = client.request("GET", "/decision/latest", json={"snapshot": snapshot})
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] in {"WAITING", "UNKNOWN", "ACCEPT", "REVIEW", "REJECT"}
