# design_brain_model/brain_model/view_model.py
"""
Phase 17-3: ViewModel Definitions
This module defines the ViewModels, which are immutable, UI-independent projections
of the domain state (SemanticUnits). They are safe, read-only data structures
intended for rendering or as input to Agents, without containing any domain logic.
"""

from dataclasses import dataclass
from typing import List, Dict, Any, Optional
from enum import Enum

# --- Enums for ViewModel Statuses ---

class L1ClusterStatus(str, Enum):
    """Status of an L1 Cluster for display purposes."""
    CREATED = "CREATED"
    ACTIVE = "ACTIVE"
    STALE = "STALE"
    RESOLVED = "RESOLVED"
    ARCHIVED = "ARCHIVED"

class DecisionPolarityVM(str, Enum):
    """Display representation of a decision's polarity."""
    ACCEPT = "ACCEPT"
    REVIEW = "REVIEW"
    REJECT = "REJECT"

# --- L1 ViewModels ---

@dataclass(frozen=True)
class L1AtomVM:
    """ViewModel for a single SemanticUnitL1."""
    id: str
    type: str
    content: str
    source: str
    timestamp: float
    referenced_in_l2_count: int

@dataclass(frozen=True)
class L1ClusterVM:
    """ViewModel for an L1 Cluster."""
    id: str
    status: L1ClusterStatus
    l1_count: int
    entropy: float  # Pre-calculated value from the domain

# --- L2 ViewModels ---

@dataclass(frozen=True)
class DecisionChipVM:
    """
    The primary and most important ViewModel for an L2 Decision.
    This is the only L2 representation that the UI/Agent should interact with.
    """
    l2_decision_id: str
    head_generation_id: str
    polarity: DecisionPolarityVM
    scope: Dict[str, Any]
    confidence: float  # Finalized value
    entropy: float     # Finalized value

@dataclass(frozen=True)
class DecisionGenerationVM:
    """Read-only ViewModel for a single generation in a decision's history."""
    generation_id: str
    timestamp: float # Placeholder, assuming L2 has a timestamp
    source_l1_count: int

@dataclass(frozen=True)
class DecisionHistoryVM:
    """Optional ViewModel to display the history of a decision."""
    l2_decision_id: str
    generations: List[DecisionGenerationVM]

# --- Context Snapshot ViewModel ---

@dataclass(frozen=True)
class L1ContextSnapshotVM:
    """
    A snapshot of the current L1 context, intended as a safe input for an Agent.
    """
    focused_cluster_id: Optional[str]
    active_l1_atoms: List[L1AtomVM]  # Limited number
    missing_types: List[str]         # e.g., ["REQUIREMENT", "EVIDENCE"]
    entropy_summary: float
