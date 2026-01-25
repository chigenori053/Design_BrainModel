from typing import List, Optional, Dict, Set
import uuid
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

class SemanticUnitKind(str, Enum):
    REQUIREMENT = "requirement"
    CONSTRAINT = "constraint"
    ASSUMPTION = "assumption"
    DECISION = "decision"
    QUESTION = "question"

class SemanticUnitStatus(str, Enum):
    UNSTABLE = "unstable"
    REVIEW = "review"
    STABLE = "stable"
    REJECTED = "rejected"

class SemanticUnit(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    kind: SemanticUnitKind
    content: str
    status: SemanticUnitStatus = SemanticUnitStatus.UNSTABLE
    confidence: float = 1.0
    dependencies: Set[str] = Field(default_factory=set) # Set of UUIDs
    resolves: Set[str] = Field(default_factory=set) # Set of UUIDs
    origin_event_id: Optional[str] = None
    last_updated_event_id: Optional[str] = None
    source_message_id: Optional[str] = None # Keeping for legacy/traceability

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

# --- Phase 2: Decision Intelligence ---

class UtilityVector(BaseModel):
    performance: float = Field(default=0.0, ge=0.0, le=1.0)
    cost: float = Field(default=0.0, ge=0.0, le=1.0)
    maintainability: float = Field(default=0.0, ge=0.0, le=1.0)
    scalability: float = Field(default=0.0, ge=0.0, le=1.0)
    risk: float = Field(default=0.0, ge=0.0, le=1.0)
    
    # Phase 2.1: Attribution
    evaluated_by: Optional[Role] = None

class Policy(BaseModel):
    name: str
    weights: Dict[str, float] # e.g. {"performance": 0.5, "cost": 0.5}

class DecisionCandidate(BaseModel):
    candidate_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    resolves_question_id: str # The Question Unit ID this candidate resolves
    content: str
    supporting_units: List[str] = [] # List of SemanticUnit UUIDs
    opposing_units: List[str] = []
    proposed_by: Role
    
    # Computed during evaluation
    utility: Optional[UtilityVector] = None 

# Phase 2.1: Traceability Record
class RankedCandidate(BaseModel):
    candidate_id: str
    content: str
    final_score: float
    utility_vector_snapshot: UtilityVector

# --- Phase 3: Consensus & Re-evaluation ---

class ConsensusStatus(str, Enum):
    ACCEPT = "ACCEPT"
    REVIEW = "REVIEW"
    REJECT = "REJECT"
    ESCALATE = "ESCALATE"

class EvaluationResult(BaseModel):
    evaluator_id: str
    candidates: List[str] # List of candidate IDs
    utility_vector: UtilityVector
    confidence: float = Field(default=0.0, ge=0.0, le=1.0)
    entropy: float = Field(default=0.0, ge=0.0)
    timestamp: datetime = Field(default_factory=datetime.now)

# Alias for generic use
Evaluation = EvaluationResult

class DecisionOutcome(BaseModel):
    outcome_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    resolves_question_id: str
    
    # Phase 2.1: Traceability Fields
    policy_id: Optional[str] = None # UUID if Policy has one, or transient ID
    policy_snapshot: Dict[str, float] = {}
    
    # Phase 3: Concensus Fields
    evaluations: List[EvaluationResult] = []
    consensus_status: Optional[ConsensusStatus] = None
    lineage: Optional[str] = None # Parent DecisionOutcome ID (if re-evaluated)
    human_reason: Optional[str] = None # If HITL
    
    ranked_candidates: List[RankedCandidate]
    explanation: str
    timestamp: datetime = Field(default_factory=datetime.now)

class DecisionState(BaseModel):
    # Active candidates being evaluated, grouped by Question ID
    candidates: Dict[str, List[DecisionCandidate]] = {} 
    
    # History of outcomes
    outcomes: List[DecisionOutcome] = []

class VMState(BaseModel):
    conversation: ConversationState = Field(default_factory=ConversationState)
    semantic_units: SemanticUnitState = Field(default_factory=SemanticUnitState)
    system_structure: SystemStructureState = Field(default_factory=SystemStructureState)
    simulation: SimulationState = Field(default_factory=SimulationState)
    execution_feedback: ExecutionFeedbackState = Field(default_factory=ExecutionFeedbackState)
    decision_state: DecisionState = Field(default_factory=DecisionState)
