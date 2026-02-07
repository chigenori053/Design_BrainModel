from __future__ import annotations
from enum import Enum
from typing import List, Dict, Any, Optional, Set
from pydantic import BaseModel, Field, ConfigDict
from ..memory.types import SemanticUnitL2, Stability
import uuid
import time

class DialoguePhase(str, Enum):
    INTAKE = "INTAKE"
    CLARIFYING = "CLARIFYING"
    STABLE = "STABLE"
    CANDIDATES_READY = "CANDIDATES_READY"
    READONLY = "READONLY"

class DesignCandidate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    label: str
    design_intent: str
    abstract_structure: Dict[str, Any]
    key_decisions: List[str]
    tradeoffs: List[str]
    assumptions: List[str]
    alignment: Dict[str, bool] # objective / constraints / scope_out との整合性

class HumanOverrideAction(str, Enum):
    ACCEPT = "ACCEPT"
    HOLD = "HOLD"
    REJECT = "REJECT"
    SAVE_AS_KNOWLEDGE = "SAVE_AS_KNOWLEDGE"

class HumanOverrideLog(BaseModel):
    unit_id: str
    candidate_id: Optional[str]
    action: HumanOverrideAction
    timestamp: float = Field(default_factory=lambda: float(time.time()))
    reason: Optional[str] = None

class DecomposedElements(BaseModel):
    objective: Optional[str] = None
    scope_in: Optional[List[str]] = None
    scope_out: Optional[List[str]] = None
    constraints: Optional[List[str]] = None
    assumptions: Optional[List[str]] = None
    success_criteria: Optional[List[str]] = None
    risks: Optional[str] = None

class ReadinessReport(BaseModel):
    stability: Stability
    satisfied_requirements: List[str] = Field(default_factory=list)
    missing_requirements: List[str] = Field(default_factory=list)
    blocking_issues: List[str] = Field(default_factory=list)

class QuestionType(str, Enum):
    SELECT = "SELECT"
    YESNO = "YESNO"
    FILL = "FILL"
    RANGE = "RANGE"

class QuestionPriority(str, Enum):
    HIGH = "HIGH"
    MEDIUM = "MEDIUM"
    LOW = "LOW"

class QuestionTemplate(BaseModel):
    id: str
    target_field: str
    type: QuestionType
    prompt: str
    options: Optional[List[str]] = None
    priority: QuestionPriority = QuestionPriority.MEDIUM

class DialogueState(BaseModel):
    semantic_unit: SemanticUnitL2
    readiness: ReadinessReport
    open_questions: List[QuestionTemplate] = Field(default_factory=list)
    candidates: List[DesignCandidate] = Field(default_factory=list)
    phase: DialoguePhase = DialoguePhase.INTAKE
