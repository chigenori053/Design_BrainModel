from typing import List, Optional, Dict
from enum import Enum
from pydantic import BaseModel, Field
from datetime import datetime

class Role(str, Enum):
    USER = "user"
    SYSTEM = "system"
    BRAIN = "brain"

class Message(BaseModel):
    id: str
    role: Role
    content: str
    timestamp: datetime = Field(default_factory=datetime.now)

class ConversationState(BaseModel):
    history: List[Message] = []

class SemanticUnitType(str, Enum):
    CONCEPT = "concept"
    CONSTRAINT = "constraint"
    BEHAVIOR = "behavior"

class SemanticUnit(BaseModel):
    id: str
    type: SemanticUnitType
    content: str
    source_message_id: Optional[str] = None
    confidence: float = 1.0

class SemanticUnitState(BaseModel):
    units: Dict[str, SemanticUnit] = {}

class SystemStructureState(BaseModel):
    # Phase 0: Simple list of components or nodes
    components: List[str] = []

class SimulationState(BaseModel):
    is_running: bool = False
    last_result: Optional[str] = None

class ExecutionFeedbackState(BaseModel):
    last_error: Optional[str] = None
    error_type: Optional[str] = None # "implementation" or "design"

class VMState(BaseModel):
    conversation: ConversationState = Field(default_factory=ConversationState)
    semantic_units: SemanticUnitState = Field(default_factory=SemanticUnitState)
    system_structure: SystemStructureState = Field(default_factory=SystemStructureState)
    simulation: SimulationState = Field(default_factory=SimulationState)
    execution_feedback: ExecutionFeedbackState = Field(default_factory=ExecutionFeedbackState)
