from enum import Enum
from datetime import datetime
from pydantic import BaseModel
from typing import Optional, Dict, Any

class Actor(str, Enum):
    USER = "user"
    DESIGN_BRAIN = "design_brain"
    EXECUTION_LAYER = "execution_layer"

class EventType(str, Enum):
    USER_INPUT = "user_input"
    EXECUTION_REQUEST = "execution_request"
    EXECUTION_RESULT = "execution_result"
    DECISION_MADE = "decision_made"
    HUMAN_OVERRIDE = "human_override"
    REQUEST_REEVALUATION = "request_reevaluation"
    VM_TERMINATE = "vm_terminate"

class BaseEvent(BaseModel):
    type: EventType
    payload: Dict[str, Any]
    actor: Optional[Actor] = None
    event_id: Optional[str] = None
    parent_event_id: Optional[str] = None
    vm_id: Optional[str] = None
    logical_index: Optional[int] = None
    wall_timestamp: Optional[datetime] = None

class UserInputEvent(BaseEvent):
    type: EventType = EventType.USER_INPUT
    actor: Actor = Actor.USER
    # payload: {"content": str}

class ExecutionResultEvent(BaseEvent):
    type: EventType = EventType.EXECUTION_RESULT
    # payload: {"success": bool, "error": str, "error_type": "implementation|design"} 

class ExecutionRequestEvent(BaseEvent):
    type: EventType = EventType.EXECUTION_REQUEST
    actor: Actor = Actor.USER

class DecisionMadeEvent(BaseEvent):
    type: EventType = EventType.DECISION_MADE
    actor: Actor = Actor.DESIGN_BRAIN

class HumanOverrideEvent(BaseEvent):
    type: EventType = EventType.HUMAN_OVERRIDE
    actor: Actor = Actor.USER

class RequestReevaluationEvent(BaseEvent):
    type: EventType = EventType.REQUEST_REEVALUATION
    actor: Actor = Actor.USER

class VmTerminateEvent(BaseEvent):
    type: EventType = EventType.VM_TERMINATE
    actor: Actor = Actor.USER
