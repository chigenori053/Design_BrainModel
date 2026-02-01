import pytest
import numpy as np
from design_brain_model.brain_model.memory.types import SemanticUnit, MemoryStatus
from design_brain_model.brain_model.memory.space import MemorySpace
from design_brain_model.brain_model.memory.strategy import RecallPhase

@pytest.fixture
def space(tmp_path):
    return MemorySpace(persistence_root=str(tmp_path))

def test_recall_phase1_canonical_only(space):
    # Setup: Canonical memory with high resonance
    v = np.array([1.0, 0.0])
    u1 = SemanticUnit(id="c1", content="canonical", type="concept")
    space.canonical.add(u1, vector=v)
    
    # Recall with high resonance, should satisfy P1 and stop
    results = space.recall(query_vector=v, entropy=0.1)
    
    assert len(results) == 1
    assert results[0]["memory_id"] == "c1"
    assert results[0]["phase"] == RecallPhase.PHASE_1

def test_recall_phase2_quarantine_triggered_by_entropy(space):
    # Setup: Canonical is weak, Quarantine is strong
    v_c = np.array([1.0, 0.0])
    v_q = np.array([0.0, 1.0])
    
    u_c = SemanticUnit(id="c_weak", content="weak canonical", type="concept")
    space.canonical.add(u_c, vector=v_c)
    
    u_q = SemanticUnit(id="q_strong", content="strong quarantine", type="concept")
    u_q.avg_EU_delta = 0.2
    space.quarantine.add(u_q, vector=v_q)
    
    # Query for Quarantine vector with high entropy
    # P1 will have resonance 0 (below theta_sim), should move to P2
    results = space.recall(query_vector=v_q, entropy=0.5)
    
    # Check if Quarantine item is included
    assert any(r["memory_id"] == "q_strong" for r in results)
    assert any(r["phase"] == RecallPhase.PHASE_2 for r in results)

def test_recall_phase3_frozen_triggered(space):
    v = np.array([1.0, 0.0])
    u_f = SemanticUnit(id="f1", content="frozen", type="concept")
    u_f.status = MemoryStatus.FROZEN
    u_f.avg_EU_delta = 0.1
    space.quarantine.add(u_f, vector=v)
    
    # Case 1: High entropy but no human/debug -> No frozen recall
    results_no_debug = space.recall(query_vector=v, entropy=0.9, debug_mode=False)
    assert len(results_no_debug) == 0
    
    # Case 2: High entropy AND debug_mode -> Frozen recall allowed
    results_debug = space.recall(query_vector=v, entropy=0.9, debug_mode=True)
    assert len(results_debug) == 1
    assert results_debug[0]["memory_id"] == "f1"
    assert results_debug[0]["phase"] == RecallPhase.PHASE_3

def test_recall_excludes_disabled(space):
    v = np.array([1.0, 0.0])
    u_d = SemanticUnit(id="d1", content="disabled", type="concept")
    u_d.status = MemoryStatus.DISABLED
    space.quarantine.add(u_d, vector=v)
    
    results = space.recall(query_vector=v, entropy=1.0, debug_mode=True, human_override=True)
    # Even in debug/override, DISABLED should never be recalled
    assert len(results) == 0

def test_recall_eu_delta_filtering(space):
    v = np.array([1.0, 0.0])
    u_low_eu = SemanticUnit(id="q_low", content="low eu", type="concept")
    u_low_eu.avg_EU_delta = -0.1 # Negative EU delta
    space.quarantine.add(u_low_eu, vector=v)
    
    results = space.recall(query_vector=v, entropy=1.0)
    assert len(results) == 0 # EU_delta <= 0 should be discarded
