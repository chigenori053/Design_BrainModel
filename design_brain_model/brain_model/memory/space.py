# design_brain_model/brain_model/memory/space.py

from typing import Dict, List, Optional
import uuid
import time
from pathlib import Path

# Import domain types from this level
from .types import SemanticUnitL1, SemanticUnitL2, L1Cluster

# Import stores
from .store import CanonicalStore, QuarantineStore, WorkingMemory

# Import commands
from command import (
    AnyCommand,
    CreateL1AtomCommand,
    CreateL1ClusterCommand,
    ArchiveL1ClusterCommand,
    ConfirmDecisionCommand,
    UpdateDecisionCommand
)

# Import view model types from the parent brain_model package
from ..view_model import (
    L1AtomVM,
    L1ClusterVM,
    L1ClusterStatus,
    DecisionChipVM,
    DecisionPolarityVM,
    L1ContextSnapshotVM,
    DecisionHistoryVM,
    DecisionGenerationVM
)

class MemorySpace:
    """
    The container for all semantic units and their clusters.
    Refactored for Phase 17-3 to manage L1/L2 units and project them to ViewModels.
    Also holds the physical Memory Stores (Spec-01).
    """
    def __init__(self, persistence_root: str = "memory_store"):
        self.l1_units: Dict[str, SemanticUnitL1] = {}
        self.l2_units: Dict[str, SemanticUnitL2] = {}
        self.l1_clusters: Dict[str, L1Cluster] = {}
        # A simplified representation of L2 Decisions. A full implementation
        # would group L2 units by a shared `decision_id`.
        self.l2_decisions: Dict[str, List[SemanticUnitL2]] = {}

        # Spec-01 Stores
        root = Path(persistence_root)
        self.canonical = CanonicalStore(root)
        self.quarantine = QuarantineStore(root)
        self.working = WorkingMemory()
        
        # Backward compatibility for gate.py (temporary)
        # gate.py uses self.phs.store(unit)
        # We'll map phs to canonical for now, but gate.py needs logic update.
        self.phs = self.canonical 

    # --- Domain Object Management (for setup and internal logic) ---

    def add_l1_unit(self, unit: SemanticUnitL1):
        """Adds an L1 unit to the memory space."""
        self.l1_units[unit.id] = unit

    def add_l2_unit(self, unit: SemanticUnitL2, decision_id: str):
        """Adds an L2 unit and associates it with a decision."""
        if unit.id in self.l2_units:
            return # Avoid duplicates

        self.l2_units[unit.id] = unit
        if decision_id not in self.l2_decisions:
            self.l2_decisions[decision_id] = []
        
        # Keep generations ordered (assuming append order is chronological)
        self.l2_decisions[decision_id].append(unit)
        
        # Update L1 units that are now referenced by this new L2
        for l1_id in unit.source_l1_ids:
            if l1_id in self.l1_units:
                # Avoid adding duplicate references
                if unit.id not in self.l1_units[l1_id].used_in_l2_ids:
                    self.l1_units[l1_id].used_in_l2_ids.append(unit.id)

    def add_cluster(self, cluster: L1Cluster):
        """Adds an L1 cluster to the memory space."""
        self.l1_clusters[cluster.id] = cluster

    # --- ViewModel Projection Functions (Phase 17-3) ---

    def project_to_l1_atom_vm(self, l1_id: str) -> Optional[L1AtomVM]:
        """Projects a SemanticUnitL1 to its L1AtomVM representation."""
        unit = self.l1_units.get(l1_id)
        if not unit:
            return None
        
        return L1AtomVM(
            id=unit.id,
            type=unit.type,
            content=unit.content,
            source=unit.source,
            timestamp=unit.timestamp,
            referenced_in_l2_count=len(unit.used_in_l2_ids)
        )
    
    def project_to_l1_cluster_vm(self, cluster_id: str) -> Optional[L1ClusterVM]:
        """Projects an L1Cluster to its L1ClusterVM representation."""
        cluster = self.l1_clusters.get(cluster_id)
        if not cluster:
            return None

        # Domain logic to determine status and entropy would exist here.
        # For Phase 17-3, we'll use placeholder values as per the spec.
        status = L1ClusterStatus.ACTIVE
        entropy = 0.75 # Placeholder

        return L1ClusterVM(
            id=cluster.id,
            status=status,
            l1_count=len(cluster.l1_ids),
            entropy=entropy
        )

    def project_to_decision_chip_vm(self, decision_id: str) -> Optional[DecisionChipVM]:
        """Projects the head of an L2 decision to its DecisionChipVM representation."""
        decision_gens = self.l2_decisions.get(decision_id)
        if not decision_gens:
            return None

        # The "HEAD GEN" is the latest generation.
        head_gen = decision_gens[-1]

        # Polarity mapping
        if head_gen.decision_polarity:
            polarity_vm = DecisionPolarityVM.ACCEPT
        else:
            # Simplified mapping, assuming False could be REVIEW or REJECT.
            # Defaulting to REJECT as per the VM enum.
            polarity_vm = DecisionPolarityVM.REJECT

        # Placeholder for confidence/entropy calculation from domain logic
        confidence = 0.95 
        entropy = 0.05

        return DecisionChipVM(
            l2_decision_id=decision_id,
            head_generation_id=head_gen.id,
            polarity=polarity_vm,
            scope=head_gen.scope,
            confidence=confidence,
            entropy=entropy
        )

    # --- Command Execution (Phase 17-4) ---

    def execute_command(self, command: AnyCommand) -> Optional[Any]:
        """
        Executes a given command, mutating the domain state.
        This is the sole entry point for writes.
        """
        if isinstance(command, CreateL1AtomCommand):
            content = (command.content or "").strip()
            if not content:
                raise ValueError("L1 content cannot be empty.")
            allowed_types = {"OBSERVATION", "REQUIREMENT", "CONSTRAINT", "HYPOTHESIS", "QUESTION"}
            if command.l1_type not in allowed_types:
                raise ValueError(f"Invalid L1 type: {command.l1_type}")
            new_id = str(uuid.uuid4())
            new_unit = SemanticUnitL1(
                id=new_id,
                content=content,
                type=command.l1_type,
                source=command.source,
                timestamp=time.time()
            )
            self.add_l1_unit(new_unit)
            return new_id
        
        elif isinstance(command, CreateL1ClusterCommand):
            # Basic validation
            if not all(l1_id in self.l1_units for l1_id in command.l1_ids):
                raise ValueError("One or more L1 IDs not found in memory space.")
            
            new_id = f"cluster-{str(uuid.uuid4())[:8]}"
            new_cluster = L1Cluster(id=new_id, l1_ids=command.l1_ids)
            self.add_cluster(new_cluster)
            return new_id

        elif isinstance(command, ArchiveL1ClusterCommand):
            if command.cluster_id in self.l1_clusters:
                # For now, we just delete it. A real implementation might
                # move it to an archived state.
                del self.l1_clusters[command.cluster_id]
                return command.cluster_id
            return None # Cluster not found

        elif isinstance(command, (ConfirmDecisionCommand, UpdateDecisionCommand)):
            # Both commands create a new L2 generation
            new_gen_id = str(uuid.uuid4())
            new_l2_gen = SemanticUnitL2(
                id=new_gen_id,
                decision_polarity=command.decision_polarity,
                evaluation=getattr(command, 'evaluation', {}), # Confirm has eval
                scope=command.scope,
                source_cluster_id=command.source_cluster_id,
                source_l1_ids=command.source_l1_ids
            )
            self.add_l2_unit(new_l2_gen, command.decision_id_to_update)
            return new_gen_id

        else:
            raise TypeError(f"Unknown command type: {type(command)}")
