from enum import Enum
from pydantic import BaseModel
from typing import Optional, Dict, Any

class EventType(str, Enum):
    USER_INPUT = "user_input"
    SEMANTIC_EXTRACT = "semantic_extract"
    STRUCTURE_EDIT = "structure_edit"
    SIMULATION_REQUEST = "simulation_request"
    EXECUTION_RESULT = "execution_result"

class BaseEvent(BaseModel):
    type: EventType
    payload: Dict[str, Any]

class UserInputEvent(BaseEvent):
    type: EventType = EventType.USER_INPUT
    # payload: {"content": str}

class SemanticExtractEvent(BaseEvent):
    type: EventType = EventType.SEMANTIC_EXTRACT
    # payload: {"units": List[SemanticUnit]}

class StructureEditEvent(BaseEvent):
    type: EventType = EventType.STRUCTURE_EDIT
    # payload: {"action": "add|remove", "component": str}

class ExecutionResultEvent(BaseEvent):
    type: EventType = EventType.EXECUTION_RESULT
    # payload: {"success": bool, "error": str, "error_type": "implementation|design"} 
