from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional
import uuid
from datetime import datetime


class IntegrationStateName(str, Enum):
    IDLE = "IDLE"
    INPUT_RECEIVED = "INPUT_RECEIVED"
    ABSTRACT_ANALYSIS = "ABSTRACT_ANALYSIS"
    FRAMING_REQUIRED = "FRAMING_REQUIRED"
    ANALYSIS_RUNNING = "ANALYSIS_RUNNING"
    INTEGRATED = "INTEGRATED"
    EVALUATED = "EVALUATED"


class SemanticUnitKind(str, Enum):
    FACT = "FACT"
    OBJECTIVE = "OBJECTIVE"
    SCOPE = "SCOPE"
    CONSTRAINT = "CONSTRAINT"
    ASSUMPTION = "ASSUMPTION"
    OTHER = "OTHER"


class DesignStructureKind(str, Enum):
    FUNCTIONAL_DECOMPOSITION = "FUNCTIONAL_DECOMPOSITION"
    LAYERING = "LAYERING"
    COMPONENT_CANDIDATE = "COMPONENT_CANDIDATE"
    OTHER = "OTHER"


class IssueType(str, Enum):
    MISSING = "missing"
    CONFLICT = "conflict"
    DEPENDENCY = "dependency"
    AMBIGUITY = "ambiguity"


class Severity(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"


@dataclass(frozen=True)
class SemanticUnit:
    id: str
    content: str
    kind: SemanticUnitKind
    source_text_id: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class DesignStructureUnit:
    id: str
    hypothesis: str
    kind: DesignStructureKind
    source_text_id: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)


class IntegrationAlignment(str, Enum):
    ALIGNED = "ALIGNED"
    MISMATCH = "MISMATCH"
    UNMAPPED = "UNMAPPED"


class DeepeningLevel(str, Enum):
    VISION = "VISION"
    FRAMED_CONCEPT = "FRAMED_CONCEPT"
    DESIGNABLE_STRUCTURE = "DESIGNABLE_STRUCTURE"


@dataclass(frozen=True)
class IntegrationMappingUnit:
    id: str
    alignment: IntegrationAlignment
    description: str
    semantic_unit_ids: List[str] = field(default_factory=list)
    structure_unit_ids: List[str] = field(default_factory=list)
    evidence: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class EvaluationSemanticUnit:
    id: str
    issue_type: IssueType
    severity: Severity
    description: str
    source_unit_ids: List[str] = field(default_factory=list)
    evidence: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class DesignSuggestionUnit:
    id: str
    prompt: str
    related_issue_ids: List[str] = field(default_factory=list)
    options: List[str] = field(default_factory=list)
    tags: List[str] = field(default_factory=list)


@dataclass(frozen=True)
class SourceRef:
    url: str
    title: str
    excerpt: str
    retrieved_at: str


@dataclass(frozen=True)
class DesignKnowledgeUnit:
    id: str
    query: str
    content_summary: str
    relevance: float
    confidence: float
    source_refs: List[SourceRef]
    notes: str = ""


@dataclass
class IntegrationState:
    state: IntegrationStateName = IntegrationStateName.IDLE
    ready_for_design: bool = False
    missing_required: List[str] = field(default_factory=list)
    conflicts: List[str] = field(default_factory=list)
    high_severity_present: bool = False
    external_knowledge_need_score: int = 0
    abstractness_score: int = 0
    framing_required: bool = False
    deepening_level: DeepeningLevel = DeepeningLevel.VISION


@dataclass(frozen=True)
class FramingFeedbackUnit:
    id: str
    abstract_score: int
    missing_elements: List[str]
    clarification_questions: List[str]
    explanation: str
    deepening_level: DeepeningLevel


@dataclass
class OrchestrationResult:
    state: IntegrationState
    semantic_units: List[SemanticUnit] = field(default_factory=list)
    structure_units: List[DesignStructureUnit] = field(default_factory=list)
    integration_units: List[IntegrationMappingUnit] = field(default_factory=list)
    evaluation_units: List[EvaluationSemanticUnit] = field(default_factory=list)
    suggestion_units: List[DesignSuggestionUnit] = field(default_factory=list)
    knowledge_units: List[DesignKnowledgeUnit] = field(default_factory=list)
    framing_feedback_units: List[FramingFeedbackUnit] = field(default_factory=list)
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())


def new_id(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4()}"
