from enum import Enum, auto
from dataclasses import dataclass, field
from typing import Optional, Any, Dict
import uuid
from datetime import datetime

class SimulationError(Exception):
    pass

class AgentState(str, Enum):
    INIT = "INIT"
    IDLE = "IDLE"
    OBSERVING = "OBSERVING"
    PROPOSING = "PROPOSING"
    SIMULATING = "SIMULATING"
    RESPONDING = "RESPONDING"
    DESIGN_REVIEW = "DESIGN_REVIEW"
    TERMINATED = "TERMINATED"

class ActionType(str, Enum):
    OBSERVE = "OBSERVE"
    RESPOND = "RESPOND"
    WAIT = "WAIT"
    PROPOSE = "PROPOSE"
    SIMULATE = "SIMULATE"

    # Explicitly prohibited
    # CREATE_UNIT = "CREATE_UNIT" 
    # UPDATE_UNIT = "UPDATE_UNIT"
    # APPLY_PROPOSAL = "APPLY_PROPOSAL"
    # AUTO_CONFIRM = "AUTO_CONFIRM"
    # CONFIRM = "CONFIRM"

@dataclass
class DesignIssue:
    """
    DesignIssue: A factual point for human discussion.
    NOT a problem to be fixed by the agent.
    NOT including solutions or recommendations.
    """
    issue_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    proposal_id: str = ""
    origin: str = "simulation" # simulation | failure_trace
    context_role: str = "STRUCTURE_CHECKER"
    issue_type: str = "uncertainty" # dependency_break | ambiguity | missing_unit | uncertainty
    description: str = ""
    evidence: dict = field(default_factory=dict) # {simulation_id, trace_id}
    status: str = "OPEN"

@dataclass
class InputData:
    """
    Represents an input observation.
    As per Phase20-A spec: "Input is all Observation", "No command distinction".
    """
    input_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())
    content: str = ""
    source: str = "user" # or 'system'

@dataclass
class DraftUnit:
    """
    Isomorphic to SemanticUnitL2 but explicitly separate namespace.
    Transient existence only.
    """
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    type: str = "DRAFT"
    content: str = ""
    # Simplified L2 fields for draft purposes
    decision_polarity: bool = False
    evaluation: Dict[str, float] = field(default_factory=dict)

@dataclass
class ProposalUnit:
    """
    Represents a hypothetical design change.
    """
    proposal_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    base_l2_version: str = "unknown" # Hash or ID of the state this proposal is based on
    proposal_type: str = "modify" # add | modify | split | remove
    target_units: list[str] = field(default_factory=list) # IDs of L2 units targeted
    draft_units: list[DraftUnit] = field(default_factory=list)
    rationale: str = ""
    impact_scope: Dict[str, Any] = field(default_factory=dict)
    status: str = "DRAFT"

@dataclass
class SimulationParams:
    """
    Mutable parameters for simulation (Observation conditions).
    """
    simulation_scope: str = "design_structure_only" # design_structure_only | dependency_only | interface_boundary_only
    analysis_depth: str = "medium" # shallow | medium | deep
    issue_filter: list[str] = field(default_factory=lambda: ["missing_unit", "circular_dependency", "ambiguity"])

@dataclass
class FailureTrace:
    """
    Failure Trace: A disposable record for human reasoning.
    NOT knowledge. NOT for learning.
    """
    trace_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    session_id: str = "current_session"
    proposal_id: str = ""
    context_id: str = ""
    context_role: str = "STRUCTURE_CHECKER" # STRUCTURE_CHECKER | DEPENDENCY_CHECKER | RISK_SCANNER
    failure_type: str = "simulation_failed"
    failure_reason: str = ""
    parameters_snapshot: dict = field(default_factory=dict)
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())

@dataclass
class SimulationContext:
    """
    Represents a 'Shadow World' for impact prediction.
    """
    simulation_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    base_l2_version: str = "unknown"
    proposal_id: str = ""
    shadow_l2_drafts: list[DraftUnit] = field(default_factory=list)
    status: str = "RUNNING" # RUNNING | COMPLETED | FAILED
    context_role: str = "STRUCTURE_CHECKER"
    params: SimulationParams = field(default_factory=SimulationParams)

@dataclass
class AgentResponse:
    """Base class for all agent responses."""
    response_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())
    type: str = "GENERIC"

@dataclass
class ObservationSummary(AgentResponse):
    """
    Response type: Observation Summary.
    Acknowledges receipt and basic understanding/summary of the input.
    """
    summary: str = ""
    type: str = "OBSERVATION_SUMMARY"

@dataclass
class ProposalResponse(AgentResponse):
    """
    Response type: Proposal Presentation.
    """
    proposal: Optional[ProposalUnit] = None
    description: str = "" # Human readable description (non-directive)
    type: str = "PROPOSAL"

@dataclass
class SimulationIssue:
    type: str # dependency_break | ambiguity | missing_unit
    description: str

@dataclass
class SimulationResult(AgentResponse):
    """
    Response type: Simulation Result.
    """
    simulation_id: str = ""
    summary: str = ""
    detected_issues: list[SimulationIssue] = field(default_factory=list)
    uncertainty_notes: list[str] = field(default_factory=list)
    type: str = "SIMULATION_RESULT"

@dataclass
class ReviewResponse(AgentResponse):
    """
    Response type: Design Review Presentation.
    """
    proposal_id: str = ""
    issues: list[DesignIssue] = field(default_factory=list)
    type: str = "DESIGN_REVIEW"

@dataclass
class StateMessage(AgentResponse):
    """
    Response type: State Message.
    Used for status updates, errors, or lifecycle events.
    """
    state: AgentState = AgentState.IDLE
    message: str = ""
    type: str = "STATE_MESSAGE"
