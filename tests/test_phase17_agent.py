# tests/test_phase17_agent.py

import pytest
from design_brain_model.agent import Agent
from design_brain_model.brain_model.view_model import L1ContextSnapshotVM
from design_brain_model.command import CreateL1AtomCommand

@pytest.fixture
def agent() -> Agent:
    """Provides a standard Agent instance for tests."""
    return Agent()

def test_agent_suggests_observation_on_missing_types(agent: Agent):
    """
    Tests that the agent generates an OBSERVATION command when it detects
    missing information types in the context snapshot.
    """
    snapshot = L1ContextSnapshotVM(
        focused_cluster_id=None,
        active_l1_atoms=[],
        missing_types=["REQUIREMENT", "EVIDENCE"],
        entropy_summary=0.5
    )
    
    command = agent.run(snapshot)
    
    assert command is not None
    assert isinstance(command, CreateL1AtomCommand)
    assert command.type == "OBSERVATION"
    assert "missing information" in command.content
    assert "REQUIREMENT" in command.content
    assert "EVIDENCE" in command.content
    assert command.source == agent.name

def test_agent_suggests_question_on_high_entropy(agent: Agent):
    """
    Tests that the agent generates a QUESTION command when context entropy is high.
    (This test assumes the missing_types check has lower priority).
    """
    snapshot = L1ContextSnapshotVM(
        focused_cluster_id="cluster-1",
        active_l1_atoms=[],
        missing_types=[], # No missing types to ensure this rule is skipped
        entropy_summary=0.85
    )
    
    command = agent.run(snapshot)
    
    assert command is not None
    assert isinstance(command, CreateL1AtomCommand)
    assert command.type == "QUESTION"
    assert "entropy is high" in command.content
    assert "0.85" in command.content

def test_agent_returns_none_when_no_suggestions(agent: Agent):
    """
    Tests that the agent returns None when the context provides no triggers
    for suggestions.
    """
    snapshot = L1ContextSnapshotVM(
        focused_cluster_id="cluster-2",
        active_l1_atoms=[],
        missing_types=[],
        entropy_summary=0.2
    )

    command = agent.run(snapshot)
    
    assert command is None
