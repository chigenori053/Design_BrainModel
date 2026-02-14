# design_brain_model/brain_model/view_model.py
"""
Phase 17-4: ViewModel Definitions (compatible with MemorySpace and Agent)
This module defines the ViewModels as immutable, UI-independent projections
of the domain state for CLI / UI clients.
"""

from dataclasses import dataclass
from typing import List, Dict, Any, Optional
from enum import Enum

# --- Enums ---

class L1ClusterStatus(str, Enum):
    CREATED = "CREATED"
    ACTIVE = "ACTIVE"
    STALE = "STALE"
    RESOLVED = "RESOLVED"
    ARCHIVED = "ARCHIVED"

class DecisionPolarityVM(str, Enum):
    ACCEPT = "ACCEPT"
    REVIEW = "REVIEW"
    REJECT = "REJECT"

# --- L1 ViewModels ---

@dataclass(frozen=True)
class L1AtomVM:
    id: str
    type: str
    content: str
    source: str
    timestamp: float
    referenced_in_l2_count: int

@dataclass(frozen=True)
class L1ClusterVM:
    id: str
    status: L1ClusterStatus
    l1_count: int
    entropy: float

# --- L2 ViewModels ---

@dataclass(frozen=True)
class DecisionChipVM:
    l2_decision_id: str
    head_generation_id: str
    polarity: DecisionPolarityVM
    scope: Dict[str, Any]
    confidence: float
    entropy: float

@dataclass(frozen=True)
class DecisionGenerationVM:
    generation_id: str
    decision_polarity: DecisionPolarityVM
    scope: Dict[str, Any]
    source_l1_ids: List[str]
    created_at: float

@dataclass(frozen=True)
class DecisionHistoryVM:
    decision_id: str
    generations: List[DecisionGenerationVM]

# --- Context Snapshot ViewModel ---

@dataclass(frozen=True)
class L1ContextSnapshotVM:
    focused_cluster_id: Optional[str]
    active_l1_atoms: List[L1AtomVM]
    missing_types: List[str]
    entropy_summary: float
