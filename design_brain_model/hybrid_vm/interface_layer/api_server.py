import uvicorn
import logging
from fastapi import FastAPI, Body, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from typing import List, Optional, Any, Dict

from design_brain_model.hybrid_vm.core import (
    HybridVM, DecisionNotFoundError, InvalidOverridePayloadError
)
from design_brain_model.hybrid_vm.control_layer.state import VMState
from design_brain_model.hybrid_vm.events import (
    EventType,
    UserInputEvent,
    HumanOverrideEvent,
    Actor,
)

# Configure Logging
logging.basicConfig(
    filename='server.log',
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

app = FastAPI()

class ApiError(Exception):
    def __init__(self, status_code: int, error: str):
        self.status_code = status_code
        self.error = error

@app.exception_handler(ApiError)
def api_error_handler(_: Request, exc: ApiError):
    return JSONResponse(status_code=exc.status_code, content={"error": exc.error})

class EventPayload(BaseModel):
    action: str
    data: Optional[Dict[str, Any]] = None

class EventRequest(BaseModel):
    snapshot: Optional[Dict[str, Any]] = None
    payload: Optional[EventPayload] = None

class SnapshotResponse(BaseModel):
    snapshot: Dict[str, Any]

class DecisionDto(BaseModel):
    id: str
    status: str
    selected_candidate: Optional[str] = None
    evaluator_count: int
    confidence: str
    entropy: str
    explanation: str
    human_override: bool

class DecisionSummaryDto(BaseModel):
    id: str
    status: str
    is_reevaluation: bool

def require_snapshot(snapshot: Optional[Dict[str, Any]]) -> Dict[str, Any]:
    if snapshot is None:
        raise ApiError(status_code=400, error="SNAPSHOT_REQUIRED")
    if not isinstance(snapshot, dict):
        raise ApiError(status_code=409, error="SNAPSHOT_MISMATCH")
    if "snapshot_id" not in snapshot or "vm_state" not in snapshot:
        raise ApiError(status_code=409, error="SNAPSHOT_MISMATCH")
    try:
        VMState.model_validate(snapshot.get("vm_state"))
    except Exception:
        raise ApiError(status_code=409, error="SNAPSHOT_MISMATCH")
    return snapshot

@app.post("/snapshot/create", response_model=SnapshotResponse)
def create_snapshot():
    vm = HybridVM.create()
    return SnapshotResponse(snapshot=vm.build_snapshot())

@app.get("/decision/latest", response_model=DecisionDto)
def get_latest_decision(snapshot: Optional[Dict[str, Any]] = Body(default=None, embed=True)):
    snapshot_dict = require_snapshot(snapshot)
    vm = HybridVM.from_snapshot(snapshot_dict.get("vm_state", {}))
    outcomes = vm.get_state_snapshot().get("decision_state", {}).get("outcomes", [])
    if not outcomes:
        # Return a default "Wait" state if no decision yet
        return DecisionDto(
            id="waiting",
            status="WAITING",
            selected_candidate=None,
            evaluator_count=0,
            confidence="LOW",
            entropy="HIGH",
            explanation="Waiting for input...",
            human_override=False
        )
    
    last = outcomes[-1]
    
    # Map consensus status to string
    status_str = last.get("consensus_status") or "UNKNOWN"
    
    # Get selected candidate (Top 1)
    selected_candidate = None
    if last.get("ranked_candidates"):
        selected_candidate = last["ranked_candidates"][0]["content"]
        
    # Confidence aggregation (Mock logic or average)
    confidence_val = "MEDIUM"
    if last.get("evaluations"):
        # Simple average or take first
        avg_conf = sum(e["confidence"] for e in last["evaluations"]) / len(last["evaluations"])
        if avg_conf > 0.8:
            confidence_val = "HIGH"
        elif avg_conf < 0.4:
            confidence_val = "LOW"
            
    is_human_override = last.get("human_reason") is not None
    
    return DecisionDto(
        id=last["outcome_id"],
        status=status_str,
        selected_candidate=selected_candidate,
        evaluator_count=len(last.get("evaluations", [])),
        confidence=confidence_val,
        entropy="LOW", # Placeholder
        explanation=last["explanation"],
        human_override=is_human_override
    )

@app.get("/decision/history", response_model=List[DecisionSummaryDto])
def get_decision_history(snapshot: Optional[Dict[str, Any]] = Body(default=None, embed=True)):
    snapshot_dict = require_snapshot(snapshot)
    vm = HybridVM.from_snapshot(snapshot_dict.get("vm_state", {}))
    outcomes = vm.get_state_snapshot().get("decision_state", {}).get("outcomes", [])
    history = []
    for o in outcomes:
        status_str = o.get("consensus_status") or "UNKNOWN"
        history.append(DecisionSummaryDto(
            id=o["outcome_id"],
            status=status_str,
            is_reevaluation=o.get("lineage") is not None
        ))
    return history

@app.post("/event")
def send_event(event: EventRequest):
    logger.info("Received Event")
    snapshot_dict = require_snapshot(event.snapshot)
    if not event.payload or not event.payload.action:
        raise ApiError(status_code=400, error="INVALID_PAYLOAD")

    vm = HybridVM.from_snapshot(snapshot_dict.get("vm_state", {}))
    action = event.payload.action
    data = event.payload.data or {}

    vm_event = None
    if action == "USER_INPUT":
        vm_event = UserInputEvent(
            type=EventType.USER_INPUT,
            payload={"content": data.get("content", "")},
            actor=Actor.USER
        )
    elif action == "CREATE_UNIT":
        vm_event = UserInputEvent(
            type=EventType.USER_INPUT,
            payload={"action": "create_unit", "unit": data.get("unit")},
            actor=Actor.USER
        )
    elif action == "CONFIRM_UNIT":
        vm_event = UserInputEvent(
            type=EventType.USER_INPUT,
            payload={"action": "confirm_unit", "unit_id": data.get("unit_id")},
            actor=Actor.USER
        )
    elif action == "HUMAN_OVERRIDE":
        vm_event = HumanOverrideEvent(
            type=EventType.HUMAN_OVERRIDE,
            payload=data, # Pass the whole data dict as payload
            actor=Actor.USER
        )
    else:
        raise ApiError(status_code=400, error="INVALID_ACTION")

    logger.info(f"Processing VM Event: {vm_event}")
    try:
        vm.process_event(vm_event)
    except DecisionNotFoundError:
        raise ApiError(status_code=404, error="DECISION_NOT_FOUND")
    except InvalidOverridePayloadError:
        raise ApiError(status_code=400, error="INVALID_OVERRIDE_PAYLOAD")
        
    return SnapshotResponse(snapshot=vm.build_snapshot())

if __name__ == "__main__":
    logger.info("Starting HybridVM API Server...")
    uvicorn.run(app, host="0.0.0.0", port=8000)
