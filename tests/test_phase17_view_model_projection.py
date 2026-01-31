# tests/test_phase17_view_model_projection.py

import pytest
import time

from design_brain_model.brain_model.memory.types import SemanticUnitL1, SemanticUnitL2, L1Cluster
from design_brain_model.brain_model.memory.space import MemorySpace
from design_brain_model.brain_model.view_model import (
    L1AtomVM,
    L1ClusterVM,
    L1ClusterStatus,
    DecisionChipVM,
    DecisionPolarityVM,
)

@pytest.fixture
def populated_memory_space() -> MemorySpace:
    """
    Provides a MemorySpace instance populated with a consistent set of
    domain objects for testing projections.
    """
    space = MemorySpace()

    # --- L1 Units ---
    l1_unit_1 = SemanticUnitL1("l1-001", "Observation", "Initial data point", "Source A", time.time())
    l1_unit_2 = SemanticUnitL1("l1-002", "Hypothesis", "Possible cause", "Source B", time.time())
    space.add_l1_unit(l1_unit_1)
    space.add_l1_unit(l1_unit_2)

    # --- L1 Cluster ---
    cluster_1 = L1Cluster("cluster-A", l1_ids=["l1-001", "l1-002"])
    space.add_cluster(cluster_1)

    # --- L2 Decision and Generations ---
    decision_id = "decision-123"
    
    # First generation of the decision
    l2_gen_1 = SemanticUnitL2(
        id="l2-gen-1",
        decision_polarity=False, # Initially REJECT/REVIEW
        evaluation={"confidence": 0.4},
        scope={"area": "test"},
        source_cluster_id="cluster-A",
        source_l1_ids=["l1-001"]
    )
    space.add_l2_unit(l2_gen_1, decision_id=decision_id)

    # Second, updated generation (HEAD)
    l2_gen_2 = SemanticUnitL2(
        id="l2-gen-2",
        decision_polarity=True, # Now ACCEPT
        evaluation={"confidence": 0.95},
        scope={"area": "test", "finalized": True},
        source_cluster_id="cluster-A",
        source_l1_ids=["l1-001", "l1-002"]
    )
    space.add_l2_unit(l2_gen_2, decision_id=decision_id)
    
    return space

def test_project_to_l1_atom_vm(populated_memory_space: MemorySpace):
    """Tests the projection of a single SemanticUnitL1 to an L1AtomVM."""
    domain_obj = populated_memory_space.l1_units["l1-002"]
    
    vm = populated_memory_space.project_to_l1_atom_vm("l1-002")
    
    assert vm is not None
    assert isinstance(vm, L1AtomVM)
    assert vm.id == domain_obj.id
    assert vm.content == domain_obj.content
    # l1-002 was used in l2-gen-2
    assert vm.referenced_in_l2_count == 1

def test_project_to_l1_cluster_vm(populated_memory_space: MemorySpace):
    """Tests the projection of an L1Cluster to an L1ClusterVM."""
    domain_obj = populated_memory_space.l1_clusters["cluster-A"]
    
    vm = populated_memory_space.project_to_l1_cluster_vm("cluster-A")

    assert vm is not None
    assert isinstance(vm, L1ClusterVM)
    assert vm.id == domain_obj.id
    assert vm.l1_count == len(domain_obj.l1_ids)
    assert vm.status == L1ClusterStatus.ACTIVE # Based on placeholder logic
    assert vm.entropy == 0.75 # Based on placeholder logic

def test_project_to_decision_chip_vm(populated_memory_space: MemorySpace):
    """Tests the projection of an L2 decision to a DecisionChipVM."""
    decision_id = "decision-123"
    head_gen_domain_obj = populated_memory_space.l2_decisions[decision_id][-1] # l2-gen-2

    vm = populated_memory_space.project_to_decision_chip_vm(decision_id)

    assert vm is not None
    assert isinstance(vm, DecisionChipVM)
    assert vm.l2_decision_id == decision_id
    assert vm.head_generation_id == head_gen_domain_obj.id
    assert vm.polarity == DecisionPolarityVM.ACCEPT # From the head generation
    assert vm.scope == head_gen_domain_obj.scope
    assert vm.confidence == 0.95 # Based on placeholder logic

def test_projection_is_snapshot(populated_memory_space: MemorySpace):
    """Ensures that two projections of the same object at the same state are identical."""
    vm1 = populated_memory_space.project_to_l1_atom_vm("l1-001")
    vm2 = populated_memory_space.project_to_l1_atom_vm("l1-001")
    
    assert vm1 is not None
    assert vm1 == vm2 # Dataclasses with frozen=True can be compared directly

    # For a mutable domain object, we can test that the VM does not change.
    domain_obj = populated_memory_space.l1_units["l1-001"]
    domain_obj.content = "Modified content"

    vm3 = populated_memory_space.project_to_l1_atom_vm("l1-001")
    assert vm3 is not None
    assert vm3.content == "Modified content"
    assert vm1.content != vm3.content # The new snapshot reflects the new state

def test_projection_of_nonexistent_id_returns_none(populated_memory_space: MemorySpace):
    """Ensures that projecting a non-existent ID returns None."""
    assert populated_memory_space.project_to_l1_atom_vm("non-existent-id") is None
    assert populated_memory_space.project_to_l1_cluster_vm("non-existent-cluster") is None
    assert populated_memory_space.project_to_decision_chip_vm("non-existent-decision") is None
