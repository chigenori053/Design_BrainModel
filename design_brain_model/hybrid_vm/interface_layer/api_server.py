import uvicorn
import json
import logging
from fastapi import FastAPI, HTTPException, Query
from pydantic import BaseModel
from typing import List, Optional, Any, Dict

from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.events import (
    BaseEvent,
    EventType,
    UserInputEvent,
    ExecutionRequestEvent,
    HumanOverrideEvent,
    RequestReevaluationEvent,
    VmTerminateEvent,
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

class EventRequest(BaseModel):
    type: str # USER_INPUT, EXECUTION_REQUEST, REQUEST_REEVALUATION, HUMAN_OVERRIDE, VM_TERMINATE
    payload: Optional[Dict[str, Any]] = None
    snapshot: Optional[Dict[str, Any]] = None

@app.get("/decision/latest", response_model=DecisionDto)
def get_latest_decision(snapshot: Optional[str] = Query(default=None)):
    if not snapshot:
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

    try:
        snapshot_dict = json.loads(snapshot)
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid snapshot JSON")

    vm = HybridVM.from_snapshot(snapshot_dict)
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
def get_decision_history(snapshot: Optional[str] = Query(default=None)):
    if not snapshot:
        return []

    try:
        snapshot_dict = json.loads(snapshot)
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid snapshot JSON")

    vm = HybridVM.from_snapshot(snapshot_dict)
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
    logger.info(f"Received Event: {event.type} Payload: {event.payload}")
    vm = HybridVM.from_snapshot(event.snapshot) if event.snapshot else HybridVM.create()
    
    # 1. Special Handling for Human Override (Evaluation Injection)
    if event.type == "HUMAN_OVERRIDE":
        payload = event.payload or {}
        vm_event = HumanOverrideEvent(
            type=EventType.HUMAN_OVERRIDE,
            payload={
                "decision": str(payload.get("decision", "ACCEPT")),
                "reason": str(payload.get("reason", "Manual Override")),
                "candidate_ids": payload.get("candidate_ids", []),
            },
            actor=Actor.USER,
        )
        vm.process_event(vm_event)
        outcomes = vm.get_state_snapshot().get("decision_state", {}).get("outcomes", [])
        outcome_id = outcomes[-1]["outcome_id"] if outcomes else None
        return {"accepted": True, "outcome_id": outcome_id, "snapshot": vm.get_state_snapshot()}

    # 2. Generic Event Handling
    vm_event = None
    
    if event.type == "USER_INPUT":
         payload = event.payload or {}
         vm_event = UserInputEvent(
             type=EventType.USER_INPUT,
             payload={"content": payload.get("text", "")},
             actor=Actor.USER
         )
    elif event.type == "EXECUTION_REQUEST":
         vm_event = ExecutionRequestEvent(
             type=EventType.EXECUTION_REQUEST,
             payload=event.payload or {},
             actor=Actor.USER,
         )
    elif event.type == "REQUEST_REEVALUATION":
         vm_event = RequestReevaluationEvent(
             type=EventType.REQUEST_REEVALUATION,
             payload=event.payload or {},
             actor=Actor.USER,
         )
    elif event.type == "VM_TERMINATE":
         vm_event = VmTerminateEvent(
             type=EventType.VM_TERMINATE,
             payload=event.payload or {},
             actor=Actor.USER,
         )
    else:
        # Generic fallback
        vm_event = BaseEvent(
            type=EventType.USER_INPUT,
            payload=event.payload or {},
            actor=Actor.USER
        )

    if vm_event:
        logger.info(f"Processing VM Event: {vm_event}")
        vm.process_event(vm_event)
        return {"accepted": True, "snapshot": vm.get_state_snapshot()}
    else:
        logger.error(f"Invalid Event Type: {event.type}")
        raise HTTPException(status_code=400, detail="Invalid Event Type")

if __name__ == "__main__":
    logger.info("Starting HybridVM API Server...")
    uvicorn.run(app, host="0.0.0.0", port=8000)
