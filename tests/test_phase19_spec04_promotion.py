import pytest
import numpy as np
from design_brain_model.brain_model.memory.types import SemanticUnit, MemoryStatus, DecisionLabel
from design_brain_model.brain_model.memory.store import CanonicalStore, QuarantineStore

@pytest.fixture
def temp_stores(tmp_path):
    canonical = CanonicalStore(tmp_path / "canonical")
    quarantine = QuarantineStore(tmp_path / "quarantine")
    return canonical, quarantine

def test_quarantine_metrics_update(temp_stores):
    _, quarantine = temp_stores
    unit = SemanticUnit(id="u1", content="metrics test", type="concept", confidence_init=0.5)
    quarantine.add(unit, vector=np.array([1.0, 0.0]))
    
    # First usage: ACCEPTED with EU delta 0.1
    assert quarantine.record_usage("u1", accepted=True, eu_delta=0.1) is True
    retrieved = quarantine.get("u1")
    assert retrieved.reuse_count == 1
    assert retrieved.accept_support_count == 1
    assert retrieved.avg_EU_delta == pytest.approx(0.1)
    
    # Second usage: REJECTED with EU delta -0.05
    assert quarantine.record_usage("u1", accepted=False, eu_delta=-0.05) is True
    retrieved = quarantine.get("u1")
    assert retrieved.reuse_count == 2
    assert retrieved.reject_impact_count == 1
    # Avg of 0.1 and -0.05 = 0.025
    assert retrieved.avg_EU_delta == pytest.approx(0.025)

def test_promotion_criteria_satisfied(temp_stores):
    canonical, quarantine = temp_stores
    unit = SemanticUnit(
        id="u-promoted", 
        content="success", 
        type="concept", 
        confidence_init=0.5,
        decision_label=DecisionLabel.REVIEW
    )
    vector = np.array([1.0, 0.0])
    quarantine.add(unit, vector=vector)
    
    # Satisfy criteria: 
    # reuse >= 2, accept_support >= 1, avg_EU >= 0.05, reject == 0
    quarantine.record_usage("u-promoted", accepted=True, eu_delta=0.1)
    quarantine.record_usage("u-promoted", accepted=True, eu_delta=0.1)
    
    assert quarantine.promote_to_canonical("u-promoted", canonical) is True
    
    # Check if in Canonical
    promoted = canonical.get("u-promoted")
    assert promoted is not None
    assert promoted.status == MemoryStatus.ACTIVE
    assert promoted.status_reason == "promoted_from_quarantine"
    assert promoted.confidence_init == 0.5
    assert promoted.reuse_count == 0 # Metrics reset

def test_promotion_blocked_by_reject_impact(temp_stores):
    canonical, quarantine = temp_stores
    unit = SemanticUnit(id="u-blocked", content="fail", type="concept", confidence_init=0.5)
    quarantine.add(unit, vector=np.array([1.0, 0.0]))
    
    quarantine.record_usage("u-blocked", accepted=True, eu_delta=0.2)
    quarantine.record_usage("u-blocked", accepted=False, eu_delta=0.1) # reject_impact_count = 1
    
    # Should fail due to reject_impact_count > 0
    assert quarantine.promote_to_canonical("u-blocked", canonical) is False
    assert canonical.get("u-blocked") is None

def test_promotion_blocked_by_low_confidence_init(temp_stores):
    canonical, quarantine = temp_stores
    # confidence_init < 0.40
    unit = SemanticUnit(id="u-low-conf", content="low conf", type="concept", confidence_init=0.35)
    quarantine.add(unit, vector=np.array([1.0, 0.0]))
    
    quarantine.record_usage("u-low-conf", accepted=True, eu_delta=0.2)
    quarantine.record_usage("u-low-conf", accepted=True, eu_delta=0.2)
    
    assert quarantine.promote_to_canonical("u-low-conf", canonical) is False

def test_promotion_blocked_by_status(temp_stores):
    canonical, quarantine = temp_stores
    unit = SemanticUnit(id="u-frozen", content="frozen", type="concept", confidence_init=0.5)
    quarantine.add(unit, vector=np.array([1.0, 0.0]))
    
    quarantine.record_usage("u-frozen", accepted=True, eu_delta=0.2)
    quarantine.record_usage("u-frozen", accepted=True, eu_delta=0.2)
    
    # Freeze it
    quarantine.update_status("u-frozen", MemoryStatus.FROZEN, reason="utility drop")
    
    # Promotion only for ACTIVE
    assert quarantine.promote_to_canonical("u-frozen", canonical) is False
