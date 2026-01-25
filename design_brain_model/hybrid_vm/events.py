from enum import Enum
from pydantic import BaseModel
from typing import Optional, Dict, Any

class Actor(str, Enum):
    USER = "user"
    DESIGN_BRAIN = "design_brain"
    EXECUTION_LAYER = "execution_layer"

class EventType(str, Enum):
    USER_INPUT = "user_input"
    SEMANTIC_EXTRACT = "semantic_extract"
    STRUCTURE_EDIT = "structure_edit"
    SIMULATION_REQUEST = "simulation_request"
    EXECUTION_RESULT = "execution_result"
    
    # Phase 1 Events
    SEMANTIC_UNIT_CREATED = "semantic_unit_created"
    SEMANTIC_UNIT_CONFIRMED = "semantic_unit_confirmed"
    SEMANTIC_CONFLICT_DETECTED = "semantic_conflict_detected"
    DECISION_OUTCOME_GENERATED = "decision_outcome_generated"

class BaseEvent(BaseModel):
    type: EventType
    payload: Dict[str, Any]
    actor: Optional[Actor] = None

class UserInputEvent(BaseEvent):
    type: EventType = EventType.USER_INPUT
    actor: Actor = Actor.USER
    # payload: {"content": str}

class SemanticUnitCreatedEvent(BaseEvent):
    type: EventType = EventType.SEMANTIC_UNIT_CREATED
    # payload: {"unit": SemanticUnit (dict)}

class SemanticUnitConfirmedEvent(BaseEvent):
    type: EventType = EventType.SEMANTIC_UNIT_CONFIRMED
    # payload: {"unit_id": str}

class SemanticConflictDetectedEvent(BaseEvent):
    type: EventType = EventType.SEMANTIC_CONFLICT_DETECTED
    # payload: {"conflict_type": str, "unit_ids": List[str], "reason": str}

class SemanticExtractEvent(BaseEvent):
    type: EventType = EventType.SEMANTIC_EXTRACT
    # payload: {"units": List[SemanticUnit]}

class StructureEditEvent(BaseEvent):
    type: EventType = EventType.STRUCTURE_EDIT
    # payload: {"action": "add|remove", "component": str}

    # payload: {"action": "add|remove", "component": str}

class ExecutionResultEvent(BaseEvent):
    type: EventType = EventType.EXECUTION_RESULT
    # payload: {"success": bool, "error": str, "error_type": "implementation|design"} 

class DecisionOutcomeGeneratedEvent(BaseEvent):
    type: EventType = EventType.DECISION_OUTCOME_GENERATED
    # payload: {"outcome": DecisionOutcome (dict)}
    actor: Actor = Actor.DESIGN_BRAIN # Usually produced by the brain or consensus engine
