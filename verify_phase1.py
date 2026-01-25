import uuid
from hybrid_vm.core import HybridVM
from hybrid_vm.control_layer.state import SemanticUnit, SemanticUnitKind, SemanticUnitStatus
from hybrid_vm.events import SemanticUnitCreatedEvent, SemanticUnitConfirmedEvent, EventType, Actor

def verify_phase1():
    print("=== STARTING PHASE 1 VERIFICATION ===")
    vm = HybridVM()
    
    # 1. Test Creation and Transition
    print("\n--- Test 1: Lifecycle (Unstable -> Review -> Stable) ---")
    unit1_id = str(uuid.uuid4())
    unit1_payload = {
        "unit": {
            "id": unit1_id,
            "kind": SemanticUnitKind.REQUIREMENT,
            "content": "Test Requirement 1",
            "status": SemanticUnitStatus.UNSTABLE
        }
    }
    
    # Create
    vm.process_event(SemanticUnitCreatedEvent(
        type=EventType.SEMANTIC_UNIT_CREATED, 
        payload=unit1_payload, 
        actor=Actor.DESIGN_BRAIN
    ))
    
    u1 = vm.state.semantic_units.units[unit1_id]
    assert u1.status == SemanticUnitStatus.UNSTABLE
    print("Creation Verified: UNSTABLE")

    # Confirm (-> Review)
    vm.process_event(SemanticUnitConfirmedEvent(
        type=EventType.SEMANTIC_UNIT_CONFIRMED,
        payload={"unit_id": unit1_id},
        actor=Actor.USER
    ))
    u1 = vm.state.semantic_units.units[unit1_id]
    assert u1.status == SemanticUnitStatus.REVIEW
    print("Transition 1 Verified: REVIEW")

    # Confirm (-> Stable)
    vm.process_event(SemanticUnitConfirmedEvent(
        type=EventType.SEMANTIC_UNIT_CONFIRMED,
        payload={"unit_id": unit1_id},
        actor=Actor.USER
    ))
    u1 = vm.state.semantic_units.units[unit1_id]
    assert u1.status == SemanticUnitStatus.STABLE
    print("Transition 2 Verified: STABLE")

    # 2. Test Dependency Conflict
    print("\n--- Test 2: Dependency Conflict ---")
    
    # Unit 2 (Unstable)
    unit2_id = str(uuid.uuid4())
    unit2_payload = {
        "unit": {
            "id": unit2_id,
            "kind": SemanticUnitKind.CONSTRAINT,
            "content": "Constraint upon Requirement 1",
            "status": SemanticUnitStatus.UNSTABLE
        }
    }
    vm.process_event(SemanticUnitCreatedEvent(payload=unit2_payload, actor=Actor.DESIGN_BRAIN))
    
    # Unit 3 (Unstable) depends on Unit 2 (which is Unstable)
    unit3_id = str(uuid.uuid4())
    unit3_payload = {
        "unit": {
            "id": unit3_id,
            "kind": SemanticUnitKind.DECISION,
            "content": "Decision based on Constraint",
            "status": SemanticUnitStatus.UNSTABLE,
            "dependencies": {unit2_id} # Set of UUIDs
        }
    }
    vm.process_event(SemanticUnitCreatedEvent(payload=unit3_payload, actor=Actor.DESIGN_BRAIN))
    
    # Move Unit 3 to Review (Should work, dependencies only checked for Stable transition?)
    # Spec says: "Dependency Violation" -> "A unit cannot be Stable if its dependencies are not Stable."
    # So Unstable -> Review should be fine.
    
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unit3_id}, actor=Actor.USER))
    u3 = vm.state.semantic_units.units[unit3_id]
    print(f"Unit 3 Status: {u3.status} (Expected: REVIEW)")
    assert u3.status == SemanticUnitStatus.REVIEW
    
    # Move Unit 3 to Stable (Should Fail because Unit 2 is UNSTABLE)
    print("Attempting to move Unit 3 to STABLE (Expect Conflict)...")
    vm.process_event(SemanticUnitConfirmedEvent(payload={"unit_id": unit3_id}, actor=Actor.USER))
    
    u3 = vm.state.semantic_units.units[unit3_id]
    print(f"Unit 3 Status: {u3.status}")
    
    # Check if conflict event was emitted
    conflict_events = [e for e in vm.event_log if e.type == EventType.SEMANTIC_CONFLICT_DETECTED]
    if conflict_events:
        print("Conflict Event Detected:", conflict_events[-1].payload)
        assert u3.status == SemanticUnitStatus.REVIEW # Should NOT have changed
        print("Dependency Conflict Verified: Correctly Blocked.")
    else:
        print("FAILED: No conflict detected.")
        exit(1)

    print("\n=== PHASE 1 VERIFICATION COMPLETE ===")

if __name__ == "__main__":
    verify_phase1()
