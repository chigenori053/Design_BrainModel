# design_brain_model/hybrid_vm/interface_layer/api_server.py
import uvicorn
import logging
import time
import uuid
from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field, ConfigDict
from typing import Any, Dict, Optional
from dataclasses import asdict

# --- Domain, ViewModel, and Command Imports ---
# Note: Adjusting relative paths to be robust from the project root.
from design_brain_model.brain_model.memory.space import MemorySpace
from design_brain_model.brain_model.memory.types import SemanticUnitL2
from design_brain_model.hybrid_vm.core import (
    HybridVM,
    DecisionNotFoundError,
    InvalidOverridePayloadError,
    HumanOverrideError,
)
from design_brain_model.hybrid_vm.events import (
    UserInputEvent,
    HumanOverrideEvent,
    EventType,
    Actor,
)
from design_brain_model.command import (
    CreateL1AtomCommand,
    CreateL1ClusterCommand,
    ArchiveL1ClusterCommand,
    ConfirmDecisionCommand,
    UpdateDecisionCommand,
)

# Configure Logging
logging.basicConfig(
    filename='server.log',
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# --- FastAPI App and Global Domain Instance ---
app = FastAPI(title="DesignBrainModel API - Phase 17-4")
memory_space = MemorySpace()  # Singleton instance for the application's lifecycle
logger.info("MemorySpace initialized.")
snapshot_store: Dict[str, Dict[str, Any]] = {}

# --- API Models ---
class CommandRequest(BaseModel):
    """Defines the structure for an incoming command request."""
    command_type: str = Field(..., alias="commandType", description="The type of the command to execute.")
    payload: Dict[str, Any] = Field(..., description="The data required to execute the command.")

    model_config = ConfigDict(populate_by_name=True)

class UiInputRequest(BaseModel):
    tab: str
    text: str
    context_id: Optional[str] = None
    model_config = ConfigDict(populate_by_name=True)

# Dictionary to map command type strings from the API to their Python classes.
COMMAND_MAP = {
    "CreateL1Atom": CreateL1AtomCommand,
    "CreateL1Cluster": CreateL1ClusterCommand,
    "ArchiveL1Cluster": ArchiveL1ClusterCommand,
    "ConfirmDecision": ConfirmDecisionCommand,
    "UpdateDecision": UpdateDecisionCommand,
}

def _get_snapshot_or_error(payload: Dict[str, Any]):
    snapshot = payload.get("snapshot")
    if not snapshot:
        return None, JSONResponse(status_code=400, content={"error": "SNAPSHOT_REQUIRED"})
    snapshot_id = snapshot.get("snapshot_id")
    if not snapshot_id or snapshot_id not in snapshot_store:
        return None, JSONResponse(status_code=409, content={"error": "SNAPSHOT_MISMATCH"})
    return snapshot, None

def _classify_l1_type(text: str) -> str:
    lowered = text.lower().strip()
    if not lowered:
        return "QUESTION"
    if "?" in text or "？" in text:
        return "QUESTION"
    if lowered.startswith(("why", "what", "how")) or any(k in text for k in ["なぜ", "どう", "何"]):
        return "QUESTION"
    if any(k in lowered for k in ["must", "need", "should"]) or any(k in text for k in ["必要", "べき", "要求"]):
        return "REQUIREMENT"
    if any(k in lowered for k in ["cannot", "must not", "limit"]) or any(k in text for k in ["できない", "禁止", "制約", "上限", "下限"]):
        return "CONSTRAINT"
    if any(k in lowered for k in ["maybe", "might", "hypothesis"]) or any(k in text for k in ["かもしれない", "仮説", "推測"]):
        return "HYPOTHESIS"
    if text.endswith((".", "。", "!", "！")):
        return "OBSERVATION"
    return "OBSERVATION"

# --- Endpoints ---

@app.get("/viewmodel/cluster/{cluster_id}", tags=["ViewModel"])
def get_cluster_viewmodel(cluster_id: str):
    """Retrieves the ViewModel for a specific L1 Cluster."""
    logger.info(f"Requesting ViewModel for cluster: {cluster_id}")
    vm = memory_space.project_to_l1_cluster_vm(cluster_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Cluster with id '{cluster_id}' not found.")
    return asdict(vm)

@app.get("/viewmodel/atom/{atom_id}", tags=["ViewModel"])
def get_atom_viewmodel(atom_id: str):
    """Retrieves the ViewModel for a specific L1 Atom."""
    logger.info(f"Requesting ViewModel for atom: {atom_id}")
    vm = memory_space.project_to_l1_atom_vm(atom_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Atom with id '{atom_id}' not found.")
    return asdict(vm)

@app.get("/viewmodel/decision/{decision_id}", tags=["ViewModel"])
def get_decision_viewmodel(decision_id: str):
    """Retrieves the ViewModel for a specific L2 Decision."""
    logger.info(f"Requesting ViewModel for decision: {decision_id}")
    vm = memory_space.project_to_decision_chip_vm(decision_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Decision with id '{decision_id}' not found.")
    return asdict(vm)

@app.post("/command", tags=["Command"])
def execute_command_endpoint(request: CommandRequest):
    """The sole endpoint for mutating the domain state by executing a command."""
    logger.info(f"Received command: {request.command_type} with payload {request.payload}")
    
    command_class = COMMAND_MAP.get(request.command_type)
    if not command_class:
        raise HTTPException(status_code=400, detail=f"Unknown command type: '{request.command_type}'")
    
    try:
        # Create a command instance from the payload dictionary.
        # This relies on the payload's keys matching the dataclass field names.
        command_instance = command_class(**request.payload)
    except TypeError as e:
        # This can happen if payload keys are wrong or a required key is missing.
        raise HTTPException(status_code=400, detail=f"Invalid payload for '{request.command_type}': {e}")

    try:
        result = memory_space.execute_command(command_instance)
        logger.info(f"Command '{request.command_type}' executed successfully. Result: {result}")
        return {"status": "success", "result": result}
    except (ValueError, TypeError) as e:
        logger.error(f"Command execution failed due to bad request: {e}")
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"An unexpected error occurred during command execution: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="An internal server error occurred.")

@app.post("/ui/input", tags=["UI"])
def ui_input(request: UiInputRequest):
    text = (request.text or "").strip()
    if not text:
        raise HTTPException(status_code=400, detail="TEXT_REQUIRED")

    tab = (request.tab or "").strip().upper()
    if tab not in {"FREE_NOTE", "UNDERSTANDING", "DESIGN_DRAFT"}:
        raise HTTPException(status_code=400, detail="INVALID_TAB")

    try:
        if tab in {"FREE_NOTE", "UNDERSTANDING"}:
            l1_type = "OBSERVATION" if tab == "FREE_NOTE" else _classify_l1_type(text)
            l1_id = memory_space.execute_command(CreateL1AtomCommand(
                content=text,
                l1_type=l1_type,
                source="ui_input",
                context_id=request.context_id
            ))
            l1_vm = memory_space.project_to_l1_atom_vm(l1_id)
            if l1_vm is None:
                raise HTTPException(status_code=500, detail="L1_PROJECTION_FAILED")

            message = "FreeNoteに記録しました。" if tab == "FREE_NOTE" else "Understandingタブに整理しました。"
            return {"status": "ok", "message": message, "l1": asdict(l1_vm)}

        # DESIGN_DRAFT
        l1_id = memory_space.execute_command(CreateL1AtomCommand(
            content=text,
            l1_type="HYPOTHESIS",
            source="ui_design_draft",
            context_id=request.context_id
        ))
        l1_vm = memory_space.project_to_l1_atom_vm(l1_id)
        decision_id = f"decision-{uuid.uuid4()}"
        l2 = SemanticUnitL2(
            objective=text,
            source_l1_ids=[l1_id],
        )
        memory_space.add_l2_unit(l2, decision_id)
        draft_vm = {
            "id": decision_id,
            "title": f"Draft {decision_id[-8:]}",
            "description": text,
            "source_l1_ids": [l1_id],
            "status": "DRAFT",
            "created_by": "human",
            "created_at": time.time(),
            "feedback_text": "Design Draftとして登録しました。",
        }
        return {
            "status": "ok",
            "message": "Design Draftに整理しました。",
            "l1": asdict(l1_vm) if l1_vm else None,
            "draft": draft_vm,
        }
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"UI input failed: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="UI_INPUT_FAILED")

@app.post("/snapshot/create", tags=["Snapshot"])
def create_snapshot():
    vm = HybridVM.create()
    snapshot = vm.build_snapshot()
    snapshot_store[snapshot["snapshot_id"]] = snapshot
    return {"snapshot": snapshot}

@app.post("/event", tags=["Event"])
async def process_event(request: Request):
    body = {}
    try:
        body = await request.json()
    except Exception:
        body = {}

    snapshot, error_response = _get_snapshot_or_error(body)
    if error_response:
        return error_response
    event_payload = body.get("payload") or {}
    action = event_payload.get("action")
    data = event_payload.get("data") or {}

    vm = HybridVM.from_snapshot(snapshot["vm_state"])
    try:
        if action == "USER_INPUT":
            event = UserInputEvent(type=EventType.USER_INPUT, payload=data, actor=Actor.USER)
            vm.process_event(event)
        elif action == "HUMAN_OVERRIDE":
            event = HumanOverrideEvent(type=EventType.HUMAN_OVERRIDE, payload=data, actor=Actor.USER)
            vm.process_event(event)
        else:
            return JSONResponse(status_code=400, content={"error": "INVALID_EVENT_ACTION"})
    except DecisionNotFoundError:
        return JSONResponse(status_code=404, content={"error": "DECISION_NOT_FOUND"})
    except (InvalidOverridePayloadError, HumanOverrideError):
        return JSONResponse(status_code=400, content={"error": "INVALID_OVERRIDE_PAYLOAD"})

    new_snapshot = vm.build_snapshot()
    snapshot_store[new_snapshot["snapshot_id"]] = new_snapshot
    return {"snapshot": new_snapshot}

@app.get("/decision/latest", tags=["Decision"])
async def get_latest_decision(request: Request):
    body = {}
    try:
        body = await request.json()
    except Exception:
        body = {}
    snapshot, error_response = _get_snapshot_or_error(body)
    if error_response:
        return error_response
    decision_nodes = snapshot.get("vm_state", {}).get("decision_state", {}).get("decision_nodes", {})
    if decision_nodes:
        node = decision_nodes[sorted(decision_nodes.keys())[-1]]
        status = node.get("status", "UNKNOWN")
    else:
        status = "WAITING"
    return {"status": status}

@app.get("/decision/history", tags=["Decision"])
async def get_decision_history(request: Request):
    body = {}
    try:
        body = await request.json()
    except Exception:
        body = {}
    snapshot, error_response = _get_snapshot_or_error(body)
    if error_response:
        return error_response
    decision_nodes = snapshot.get("vm_state", {}).get("decision_state", {}).get("decision_nodes", {})
    history = [
        {"decision_id": node_id, "status": node.get("status", "UNKNOWN")}
        for node_id, node in decision_nodes.items()
    ]
    return {"history": history}

if __name__ == "__main__":
    logger.info("Starting DesignBrainModel API Server (Phase 17-4)...")
    # To run this server, execute: `python -m design_brain_model.hybrid_vm.interface_layer.api_server`
    # from the root of the project, ensuring PYTHONPATH is set correctly.
    uvicorn.run("design_brain_model.hybrid_vm.interface_layer.api_server:app", host="0.0.0.0", port=8000, reload=True)
