import uuid
from hybrid_vm.core import HybridVM
from hybrid_vm.state import SemanticUnit, SemanticUnitKind, SemanticUnitStatus
from hybrid_vm.events import SemanticUnitCreatedEvent, SemanticUnitConfirmedEvent, EventType, Actor

def verify_phase1_1():
    print("=== STARTING PHASE 1.1 VERIFICATION ===")
    vm = HybridVM()
    
    # 1. Terminal State Safety (Stable -> No-Op)
    print("\n--- Test 1: Terminal State Safety (Stable Locked) ---")
    unit1_id = str(uuid.uuid4())
    unit1_payload = {
        "unit": {
            "id": unit1_id,
            "kind": SemanticUnitKind.REQUIREMENT,
            "content": "Stable Unit",
            "status": SemanticUnitStatus.STABLE # Force STABLE for test
        }
    }
    vm.process_event(SemanticUnitCreatedEvent(payload=unit1_payload, actor=Actor.DESIGN_BRAIN))
    
    # Attempt to Confirm (Should be ignored)
    print("Attempting to Confirm STABLE unit...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unit1_id}, actor=Actor.USER))
    
    u1 = vm.state.semantic_units.units[unit1_id]
    assert u1.status == SemanticUnitStatus.STABLE
    print("Verified: Unit remains STABLE (No-Op)")

    # 2. Terminal State Safety (Rejected -> Locked)
    print("\n--- Test 2: Terminal State Safety (Rejected Locked) ---")
    unit2_id = str(uuid.uuid4())
    unit2_payload = {
        "unit": {
            "id": unit2_id,
            "kind": SemanticUnitKind.CONSTRAINT,
            "content": "Rejected Constraint",
            "status": SemanticUnitStatus.REJECTED # Force REJECTED
        }
    }
    vm.process_event(SemanticUnitCreatedEvent(payload=unit2_payload, actor=Actor.DESIGN_BRAIN))
    
    # Attempt to Confirm (Should be ignored)
    print("Attempting to Confirm REJECTED unit...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unit2_id}, actor=Actor.USER))
    
    u2 = vm.state.semantic_units.units[unit2_id]
    assert u2.status == SemanticUnitStatus.REJECTED
    print("Verified: Unit remains REJECTED (No-Op)")

    # 3. Dependency Resolution Flow
    print("\n--- Test 3: Dependency Resolution Flow ---")
    
    # Unit A (The dependency)
    unitA_id = str(uuid.uuid4())
    vm.process_event(SemanticUnitCreatedEvent(payload={
        "unit": {"id": unitA_id, "kind": SemanticUnitKind.REQUIREMENT, "content": "Dependency A", "status": SemanticUnitStatus.UNSTABLE}
    }, actor=Actor.DESIGN_BRAIN))
    
    # Unit B (Depends on A)
    unitB_id = str(uuid.uuid4())
    vm.process_event(SemanticUnitCreatedEvent(payload={
        "unit": {
            "id": unitB_id, 
            "kind": SemanticUnitKind.DECISION, 
            "content": "Dependent Decision B", 
            "status": SemanticUnitStatus.UNSTABLE,
            "dependencies": {unitA_id}
        }
    }, actor=Actor.DESIGN_BRAIN))

    # Move B to REVIEW (OK)
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unitB_id}, actor=Actor.USER))
    assert vm.state.semantic_units.units[unitB_id].status == SemanticUnitStatus.REVIEW
    
    # Try moving B to STABLE (Fail -> Conflict)
    print("Attempting B -> STABLE (Should Fail due to A Unstable)...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unitB_id}, actor=Actor.USER))
    assert vm.state.semantic_units.units[unitB_id].status == SemanticUnitStatus.REVIEW # Stuck
    
    # Stabilize A (Unstable -> Review -> Stable)
    print("Stabilizing A...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unitA_id}, actor=Actor.USER)) # -> Review
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unitA_id}, actor=Actor.USER)) # -> Stable
    assert vm.state.semantic_units.units[unitA_id].status == SemanticUnitStatus.STABLE
    
    # Try moving B to STABLE again (Success)
    print("Attempting B -> STABLE (Should Success)...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unitB_id}, actor=Actor.USER))
    assert vm.state.semantic_units.units[unitB_id].status == SemanticUnitStatus.STABLE
    print("Verified: Dependency Resolution Flow Successful")

    print("\n=== PHASE 1.1 VERIFICATION COMPLETE ===")

if __name__ == "__main__":
    verify_phase1_1()
