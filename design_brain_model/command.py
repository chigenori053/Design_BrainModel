# design_brain_model/command.py
"""
Phase 17-4: Command Layer Definitions
This module defines the Command objects that represent the sole entry point
for writing or mutating the domain state. UI/Agent interactions are translated
into these explicit, verifiable commands.
"""

from dataclasses import dataclass
from typing import List, Dict, Any, Union

# --- Base Command ---

@dataclass
class Command:
    """Base class for all commands to allow for type hinting."""
    pass

# --- L1 Commands ---

@dataclass
class CreateL1AtomCommand(Command):
    """Command to create a new SemanticUnitL1."""
    content: str
    type: str
    source: str # e.g., "HumanInput", "Agent-XYZ"

@dataclass
class CreateL1ClusterCommand(Command):
    """Command to group a set of L1 atoms into a new cluster."""
    l1_ids: List[str]

@dataclass
class ArchiveL1ClusterCommand(Command):
    """Command to archive an L1 cluster."""
    cluster_id: str

# --- L2 / Decision Commands ---

@dataclass
class ConfirmDecisionCommand(Command):
    """
    Command to confirm a decision, effectively creating a new L2 generation.
    This is a Human Override-only operation.
    """
    source_cluster_id: str
    source_l1_ids: List[str]
    decision_id_to_update: str # ID of the decision being updated
    decision_polarity: bool
    evaluation: Dict[str, float]
    scope: Dict[str, Any]

@dataclass
class UpdateDecisionCommand(Command):
    """
    Command to provide a delta update to a decision.
    In this phase, its implementation can be similar to ConfirmDecisionCommand.
    This is a Human Override-only operation.
    """
    decision_id_to_update: str
    # Fields that can be updated
    decision_polarity: bool
    scope: Dict[str, Any]
    # For simplicity, we require source info even for an update
    source_cluster_id: str
    source_l1_ids: List[str]


# A union type for easier handling in the command executor
AnyCommand = Union[
    CreateL1AtomCommand,
    CreateL1ClusterCommand,
    ArchiveL1ClusterCommand,
    ConfirmDecisionCommand,
    UpdateDecisionCommand
]
