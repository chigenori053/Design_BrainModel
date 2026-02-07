from enum import Enum
from dataclasses import dataclass, field
from typing import Optional, Set, List

class Stability(Enum):
    UNSTABLE = "UNSTABLE"
    PARTIAL = "PARTIAL"
    STABLE = "STABLE"

class DialoguePhase(Enum):
    INTAKE = "INTAKE"
    CLARIFYING = "CLARIFYING"
    STABLE = "STABLE"

@dataclass
class SemanticUnitL2:
    objective: Optional[str] = None
    scope_in: List[str] = field(default_factory=list)
    scope_out: List[str] = field(default_factory=list)
    success_criteria: Optional[str] = None
    confirmed: Set[str] = field(default_factory=set)

@dataclass
class ReadinessReport:
    stability: Stability
    missing: List[str]
