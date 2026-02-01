import pytest
import numpy as np
import json
from design_brain_model.brain_model.memory.types import (
    SemanticUnit, Decision, DecisionResult, MemoryStatus
)
from design_brain_model.brain_model.memory.gate import MemoryGate
from design_brain_model.brain_model.memory.space import MemorySpace

@pytest.fixture
def memory_space(tmp_path):
    return MemorySpace(persistence_root=str(tmp_path))

@pytest.fixture
def gate(memory_space):
    return MemoryGate(memory_space)

def test_routing_accept_to_canonical(gate, memory_space):
    unit = SemanticUnit(id="u-accept", content="test accept", type="concept")
    decision = DecisionResult(
        label=Decision.ACCEPT,
        confidence=0.9,
        entropy=0.1,
        utility=0.8,
        reason="Clear consensus"
    )
    
    assert gate.process(unit, decision, vector=np.array([1.0, 0.0])) is True
    
    # Check if in CanonicalStore
    retrieved = memory_space.canonical.unit_store.get("u-accept")
    assert retrieved is not None
    assert retrieved.decision_label == Decision.ACCEPT
    assert retrieved.status == MemoryStatus.ACTIVE
    assert retrieved.confidence_init == 0.9
    assert retrieved.decision_reason == "Clear consensus"
    assert retrieved.status_reason == "initial_decision:ACCEPT"

def test_routing_review_to_quarantine(gate, memory_space):
    unit = SemanticUnit(id="u-review", content="test review", type="concept")
    decision = DecisionResult(
        label=Decision.REVIEW,
        confidence=0.5,
        entropy=0.5,
        utility=0.3,
        reason="Ambiguous"
    )
    
    assert gate.process(unit, decision) is True
    
    # Should NOT be in Canonical
    assert memory_space.canonical.unit_store.get("u-review") is None
    
    # Should be in Quarantine
    retrieved = memory_space.quarantine.unit_store.get("u-review")
    assert retrieved is not None
    assert retrieved.decision_label == Decision.REVIEW
    assert retrieved.status == MemoryStatus.ACTIVE
    assert retrieved.status_reason == "initial_decision:REVIEW"

def test_routing_reject_to_quarantine(gate, memory_space):
    unit = SemanticUnit(id="u-reject", content="test reject", type="concept")
    decision = DecisionResult(
        label=Decision.REJECT,
        confidence=0.1,
        entropy=0.9,
        utility=0.0,
        reason="Invalid"
    )
    
    assert gate.process(unit, decision) is True
    
    # Should be in Quarantine
    retrieved = memory_space.quarantine.unit_store.get("u-reject")
    assert retrieved is not None
    assert retrieved.decision_label == Decision.REJECT
    assert retrieved.status == MemoryStatus.ACTIVE

def test_gate_metadata_immutability(gate, memory_space):
    """Spec-03: Gate initializes metadata, Memory layer does not re-interpret."""
    unit = SemanticUnit(id="u-meta", content="meta test", type="concept")
    decision = DecisionResult(
        label=Decision.ACCEPT,
        confidence=0.95,
        entropy=0.05,
        utility=0.99,
        reason="High precision"
    )
    
    gate.process(unit, decision)
    
    retrieved = memory_space.canonical.unit_store.get("u-meta")
    assert retrieved.confidence_init == 0.95
    assert retrieved.decision_label == Decision.ACCEPT
    assert retrieved.decision_reason == "High precision"
