import pytest
import numpy as np
import time
from design_brain_model.brain_model.memory.types import SemanticUnit, MemoryStatus
from design_brain_model.brain_model.memory.store import CanonicalStore, QuarantineStore

def test_promotion_after_restart(tmp_path):
    """B-1: Verify that promotion works even if the store is reloaded from disk."""
    store_dir = tmp_path / "restart_test"
    quarantine = QuarantineStore(store_dir)
    canonical = CanonicalStore(store_dir)
    
    # 1. Add unit to Quarantine and satisfy criteria
    u1 = SemanticUnit(id="u_restart", content="restart test", type="concept", confidence_init=0.5)
    v1 = np.array([1.0, 0.0], dtype=np.float32)
    quarantine.add(u1, vector=v1)
    
    quarantine.record_usage("u_restart", accepted=True, eu_delta=0.1)
    quarantine.record_usage("u_restart", accepted=True, eu_delta=0.1)
    quarantine.flush()
    
    # 2. Simulate Restart (New instances)
    new_quarantine = QuarantineStore(store_dir)
    new_canonical = CanonicalStore(store_dir)
    
    # 3. Promote (B-1: get_trace_by_source_unit_id should load the vector)
    assert new_quarantine.promote_to_canonical("u_restart", new_canonical) is True
    
    # 4. Verify in Canonical
    promoted = new_canonical.get("u_restart")
    assert promoted is not None
    hits = new_canonical.recall(v1)
    assert len(hits) == 1
    assert hits[0].key == "u_restart"

def test_status_timestamp_persistence(tmp_path):
    """C-1: Verify that status_changed_at is updated and persisted."""
    store_dir = tmp_path / "status_test"
    quarantine = QuarantineStore(store_dir)
    u1 = SemanticUnit(id="u_status", content="status test", type="concept")
    quarantine.add(u1)
    
    initial_time = quarantine.get("u_status").status_changed_at
    time.sleep(0.1) # Ensure time diff
    
    quarantine.update_status("u_status", MemoryStatus.FROZEN, reason="test")
    new_time = quarantine.get("u_status").status_changed_at
    
    assert new_time > initial_time
    
    quarantine.flush()
    
    # Reload
    new_quarantine = QuarantineStore(store_dir)
    assert new_quarantine.get("u_status").status_changed_at == new_time

def test_trace_timestamps(tmp_path):
    """D-1: Verify that traces have real, non-zero timestamps."""
    store_dir = tmp_path / "trace_test"
    quarantine = QuarantineStore(store_dir)
    
    v = np.array([1.0, 0.0])
    quarantine.add(SemanticUnit(id="t1", content="t1", type="concept"), vector=v)
    time.sleep(1.1) # Resolution of timestamp is int(time)
    quarantine.add(SemanticUnit(id="t2", content="t2", type="concept"), vector=v)
    
    traces = quarantine.vector_store._traces
    assert len(traces) == 2
    assert traces[0].timestamp > 0
    assert traces[1].timestamp > traces[0].timestamp
