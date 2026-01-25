import uvicorn
import logging
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Optional, Any, Dict

from hybrid_vm.core import HybridVM
from hybrid_vm.events import BaseEvent, EventType, UserInputEvent, Actor
from hybrid_vm.control_layer.state import ConsensusStatus

# Configure Logging
logging.basicConfig(
    filename='server.log',
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

app = FastAPI()

# Global VM Instance
# In a real production app, we'd manage lifecycle better, but for this POC global is fine.
vm = HybridVM()

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
    type: str # USER_INPUT, REQUEST_REEVALUATION, HUMAN_OVERRIDE
    payload: Optional[Dict[str, Any]] = None

@app.get("/decision/latest", response_model=DecisionDto)
def get_latest_decision():
    # Fetch from VM State
    outcomes = vm.state.decision_state.outcomes
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
    status_str = last.consensus_status.value if last.consensus_status else "UNKNOWN"
    
    # Get selected candidate (Top 1)
    selected_candidate = None
    if last.ranked_candidates:
        selected_candidate = last.ranked_candidates[0].content
        
    # Confidence aggregation (Mock logic or average)
    confidence_val = "MEDIUM"
    if last.evaluations:
        # Simple average or take first
        avg_conf = sum(e.confidence for e in last.evaluations) / len(last.evaluations)
        if avg_conf > 0.8:
            confidence_val = "HIGH"
        elif avg_conf < 0.4:
            confidence_val = "LOW"
            
    is_human_override = last.human_reason is not None
    
    return DecisionDto(
        id=last.outcome_id,
        status=status_str,
        selected_candidate=selected_candidate,
        evaluator_count=len(last.evaluations),
        confidence=confidence_val,
        entropy="LOW", # Placeholder
        explanation=last.explanation,
        human_override=is_human_override
    )

@app.get("/decision/history", response_model=List[DecisionSummaryDto])
def get_decision_history():
    outcomes = vm.state.decision_state.outcomes
    history = []
    for o in outcomes:
        status_str = o.consensus_status.value if o.consensus_status else "UNKNOWN"
        history.append(DecisionSummaryDto(
            id=o.outcome_id,
            status=status_str,
            is_reevaluation=o.lineage is not None
        ))
    return history

@app.post("/event")
def send_event(event: EventRequest):
    logger.info(f"Received Event: {event.type} Payload: {event.payload}")
    vm_event = None
    
    # Simple mapping
    if event.type == "USER_INPUT":
         payload = event.payload or {}
         vm_event = UserInputEvent(
             type=EventType.USER_INPUT,
             payload={"content": payload.get("text", "")},
             actor=Actor.USER
         )
    elif event.type == "REQUEST_REEVALUATION":
         # Use generic BaseEvent if specific class not defined/imported
         vm_event = BaseEvent(
             type=EventType.SIMULATION_REQUEST, # Re-using Sim Request or define new
             payload={},
             actor=Actor.USER
         )
    elif event.type == "HUMAN_OVERRIDE":
         # Fallback to generic
         vm_event = BaseEvent(
             type=EventType.USER_INPUT, # Temporary mapping
             payload=event.payload or {},
             actor=Actor.USER
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
        return {"accepted": True}
    else:
        logger.error(f"Invalid Event Type: {event.type}")
        raise HTTPException(status_code=400, detail="Invalid Event Type")

if __name__ == "__main__":
    logger.info("Starting HybridVM API Server...")
    uvicorn.run(app, host="0.0.0.0", port=8000)
