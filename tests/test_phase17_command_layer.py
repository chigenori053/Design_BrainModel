# tests/test_phase17_command_layer.py

import pytest
from design_brain_model.brain_model.memory.space import MemorySpace
from design_brain_model.command import (
    CreateL1AtomCommand,
    CreateL1ClusterCommand,
    ArchiveL1ClusterCommand,
    ConfirmDecisionCommand,
)

@pytest.fixture
def memory_space() -> MemorySpace:
    """Provides a clean MemorySpace instance for each test."""
    return MemorySpace()

def test_execute_create_l1_atom_command(memory_space: MemorySpace):
    """Tests that CreateL1AtomCommand correctly adds an L1 unit."""
    assert len(memory_space.l1_units) == 0
    
    command = CreateL1AtomCommand(
        content="This is a test atom.",
        type="OBSERVATION",
        source="pytest"
    )
    new_id = memory_space.execute_command(command)
    
    assert len(memory_space.l1_units) == 1
    assert new_id in memory_space.l1_units
    
    new_unit = memory_space.l1_units[new_id]
    assert new_unit.content == "This is a test atom."
    assert new_unit.source == "pytest"

def test_execute_create_l1_cluster_command(memory_space: MemorySpace):
    """Tests that CreateL1ClusterCommand correctly creates a cluster."""
    # First, add some L1 units to be clustered
    cmd1 = CreateL1AtomCommand("content1", "OBSERVATION", "src1")
    id1 = memory_space.execute_command(cmd1)
    cmd2 = CreateL1AtomCommand("content2", "REQUIREMENT", "src2")
    id2 = memory_space.execute_command(cmd2)

    assert len(memory_space.l1_clusters) == 0

    cluster_command = CreateL1ClusterCommand(l1_ids=[id1, id2])
    cluster_id = memory_space.execute_command(cluster_command)

    assert len(memory_space.l1_clusters) == 1
    assert cluster_id in memory_space.l1_clusters
    
    new_cluster = memory_space.l1_clusters[cluster_id]
    assert new_cluster.l1_ids == [id1, id2]

def test_execute_archive_l1_cluster_command(memory_space: MemorySpace):
    """Tests that ArchiveL1ClusterCommand removes a cluster."""
    # Setup cluster
    cluster_cmd = CreateL1ClusterCommand(l1_ids=[])
    cluster_id = memory_space.execute_command(cluster_cmd)
    assert cluster_id in memory_space.l1_clusters

    # Execute archive command
    archive_command = ArchiveL1ClusterCommand(cluster_id=cluster_id)
    archived_id = memory_space.execute_command(archive_command)

    assert archived_id == cluster_id
    assert cluster_id not in memory_space.l1_clusters

def test_execute_confirm_decision_command(memory_space: MemorySpace):
    """Tests that ConfirmDecisionCommand creates a new L2 generation."""
    # Setup L1 units and a cluster
    id1 = memory_space.execute_command(CreateL1AtomCommand("c1", "OBSERVATION", "s1"))
    cluster_id = memory_space.execute_command(CreateL1ClusterCommand(l1_ids=[id1]))
    
    decision_id = "decision-abc"
    assert decision_id not in memory_space.l2_decisions

    decision_command = ConfirmDecisionCommand(
        decision_id_to_update=decision_id,
        source_cluster_id=cluster_id,
        source_l1_ids=[id1],
        decision_polarity=True,
        evaluation={"confidence": 0.9},
        scope={"target": "all"}
    )
    
    new_gen_id = memory_space.execute_command(decision_command)

    assert len(memory_space.l2_units) == 1
    assert new_gen_id in memory_space.l2_units
    assert decision_id in memory_space.l2_decisions
    assert len(memory_space.l2_decisions[decision_id]) == 1

    # Check that the source L1 unit was updated
    l1_unit = memory_space.l1_units[id1]
    assert new_gen_id in l1_unit.used_in_l2_ids

def test_execute_invalid_command_type(memory_space: MemorySpace):
    """Tests that an unknown command type raises a TypeError."""
    class UnknownCommand:
        pass
    
    with pytest.raises(TypeError):
        memory_space.execute_command(UnknownCommand())

def test_create_cluster_with_invalid_l1_id_fails(memory_space: MemorySpace):
    """Tests that creating a cluster with non-existent L1 IDs raises an error."""
    id1 = memory_space.execute_command(CreateL1AtomCommand("c1", "OBSERVATION", "s1"))

    invalid_cluster_command = CreateL1ClusterCommand(l1_ids=[id1, "non-existent-id"])
    
    with pytest.raises(ValueError, match="One or more L1 IDs not found"):
        memory_space.execute_command(invalid_cluster_command)
