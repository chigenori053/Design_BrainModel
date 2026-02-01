import pytest
import numpy as np
from pathlib import Path
from design_brain_model.brain_model.memory.types import SemanticUnit, MemoryStatus
from design_brain_model.brain_model.memory.store import CanonicalStore, QuarantineStore

@pytest.fixture
def temp_store_dir(tmp_path):
    return tmp_path

def test_canonical_store_only_active(temp_store_dir):
    store = CanonicalStore(temp_store_dir)
    unit = SemanticUnit(content="test", type="concept")
    
    # Try to add with different status (should be forced to ACTIVE)
    unit.status = MemoryStatus.FROZEN
    store.add(unit, vector=np.array([1.0, 0.0]))
    
    # Check if retrieved unit is ACTIVE
    retrieved = store.unit_store.get(unit.id)
    assert retrieved.status == MemoryStatus.ACTIVE
    
    # Recall should only return ACTIVE
    hits = store.recall(np.array([1.0, 0.0]))
    assert len(hits) == 1
    assert hits[0].key == unit.id

def test_quarantine_store_status_transitions(temp_store_dir):
    store = QuarantineStore(temp_store_dir)
    unit = SemanticUnit(content="quarantine test", type="concept")
    vector = np.array([0.0, 1.0])
    store.add(unit, vector=vector)
    
    # Initial status is ACTIVE
    assert store.unit_store.get(unit.id).status == MemoryStatus.ACTIVE
    
    # ACTIVE -> FROZEN (Allowed)
    assert store.update_status(unit.id, MemoryStatus.FROZEN, reason="low utility") is True
    assert store.unit_store.get(unit.id).status == MemoryStatus.FROZEN
    
    # FROZEN -> ACTIVE without human override (Forbidden)
    assert store.update_status(unit.id, MemoryStatus.ACTIVE, reason="mistake", human_override=False) is False
    assert store.unit_store.get(unit.id).status == MemoryStatus.FROZEN
    
    # FROZEN -> ACTIVE with human override (Allowed)
    assert store.update_status(unit.id, MemoryStatus.ACTIVE, reason="manual fix", human_override=True) is True
    assert store.unit_store.get(unit.id).status == MemoryStatus.ACTIVE
    
    # ACTIVE -> DISABLED (Allowed)
    assert store.update_status(unit.id, MemoryStatus.DISABLED, reason="safety") is True
    assert store.unit_store.get(unit.id).status == MemoryStatus.DISABLED
    
    # DISABLED -> ACTIVE without human override (Forbidden)
    assert store.update_status(unit.id, MemoryStatus.ACTIVE, reason="back", human_override=False) is False
    assert store.unit_store.get(unit.id).status == MemoryStatus.DISABLED

def test_recall_filters_by_status(temp_store_dir):
    store = QuarantineStore(temp_store_dir)
    
    # Unit 1: ACTIVE
    u1 = SemanticUnit(id="u1", content="active unit", type="concept")
    v1 = np.array([1.0, 0.0])
    store.add(u1, vector=v1)
    
    # Unit 2: FROZEN
    u2 = SemanticUnit(id="u2", content="frozen unit", type="concept")
    v2 = np.array([0.0, 1.0])
    store.add(u2, vector=v2)
    store.update_status("u2", MemoryStatus.FROZEN, reason="frozen")
    
    # Default recall (Only ACTIVE)
    hits = store.recall(v2, top_k=2)
    assert len(hits) == 0 # v2 is frozen, v1 is active but dissimilar (resonance 0)
    
    hits_active = store.recall(v1, top_k=2)
    assert len(hits_active) == 1
    assert hits_active[0].key == "u1"
    
    # Explicit recall for FROZEN
    hits_frozen = store.recall(v2, top_k=2, include_statuses={MemoryStatus.FROZEN})
    assert len(hits_frozen) == 1
    assert hits_frozen[0].key == "u2"

def test_persistence_of_status(temp_store_dir):
    store_dir = temp_store_dir / "persist_test"
    store = QuarantineStore(store_dir)
    u1 = SemanticUnit(id="u_persist", content="persist", type="concept")
    store.add(u1, vector=np.array([1.0, 1.0]))
    store.update_status("u_persist", MemoryStatus.DISABLED, reason="persist test")
    store.flush()
    
    # Re-load store
    new_store = QuarantineStore(store_dir)
    new_store.load()
    
    # Check status
    assert new_store.unit_store.get("u_persist").status == MemoryStatus.DISABLED
    
    # Recall (Default: ACTIVE only) should return nothing
    hits = new_store.recall(np.array([1.0, 1.0]))
    assert len(hits) == 0
    
    # Recall with DISABLED should return it
    hits_disabled = new_store.recall(np.array([1.0, 1.0]), include_statuses={MemoryStatus.DISABLED})
    assert len(hits_disabled) == 1
    assert hits_disabled[0].key == "u_persist"
