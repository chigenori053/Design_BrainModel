import sys
import os

# Ensure we can import design_brain_model
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from design_brain_model.brain_model.api import DesignBrainModel, DesignCommand, DesignCommandType
from design_brain_model.brain_model.memory.types import MemoryType, Decision, Classification

def verify_phase9():
    print("=== STARTING PHASE 9 ARCHITECTURE VERIFICATION ===")
    
    # 1. Initialize BrainModel
    brain = DesignBrainModel()
    
    # 2. Architecture Check
    print("\n[Check 1] MemorySpace Structure")
    assert brain.memory_space.phs is not None, "PHS missing"
    assert brain.memory_space.shm is not None, "SHM missing"
    assert brain.memory_space.chm is not None, "CHM missing"
    assert brain.memory_space.dhm is not None, "DHM missing"
    print("PASS: All Memory Types initialized.")

    print("\n[Check 2] DHM Inactive/Empty")
    # DHM should refuse storage in Phase 9
    from design_brain_model.brain_model.memory.types import SemanticUnit
    dummy_unit = SemanticUnit(content="Test", type="test")
    success = brain.memory_space.dhm.store(dummy_unit)
    assert success is False, "DHM should NOT accept storage in Phase 9"
    print("PASS: DHM is effectively inactive.")

    # 3. Flow Check: Core-A -> Core-B -> Gate -> PHS
    print("\n[Check 3] Processing Flow (ACCEPT path)")
    payload = {"content": "I need a Database for user data.", "message_id": "msg_123"}
    cmd = DesignCommand(type=DesignCommandType.EXTRACT_SEMANTICS, payload=payload)
    
    result = brain.handle_design_command(cmd)
    assert result.success is True
    
    # Verify Content in PHS
    # The mock logic in Core-A/B ensures "Database" -> ACCEPT -> PHS
    # Let's inspect PHS directly to verify persistence.
    phs_units = brain.memory_space.phs.list_all()
    db_units = [u for u in phs_units if "Database" in u.content]
    
    assert len(db_units) > 0, "Accepted unit 'Database' not found in PHS"
    stored_unit = db_units[0]
    assert stored_unit.decision == Decision.ACCEPT
    assert stored_unit.memory_type == MemoryType.PHS
    print(f"PASS: Unit stored in PHS: {stored_unit.content} [{stored_unit.decision}]")

    # 4. Flow Check: REJECT/Discard path
    print("\n[Check 4] Processing Flow (Discard path)")
    # "Hello" -> REJECT -> DISCARDABLE (Mock logic)
    payload_discard = {"content": "Hello World", "message_id": "msg_456"}
    brain.handle_design_command(DesignCommand(type=DesignCommandType.EXTRACT_SEMANTICS, payload=payload_discard))
    
    # Verify NOT in PHS
    phs_units_after = brain.memory_space.phs.list_all()
    hello_units = [u for u in phs_units_after if "Hello World" in u.content]
    assert len(hello_units) == 0, "Discarded unit found in PHS! MemoryGate failure."
    print("PASS: Discarded unit not found in PHS.")

    # 5. Promotion Logic (Manual)
    print("\n[Check 5] Promotion PHS -> SHM")
    # stored_unit is "Database" (Generalizable)
    assert stored_unit.classification == Classification.GENERALIZABLE
    
    # Verify SHM is empty initially
    assert len(brain.memory_space.shm.list_all()) == 0
    
    # Promote
    promoted = brain.memory_space.promote_to_shm(stored_unit)
    assert promoted is True
    
    # Verify SHM has content
    shm_units = brain.memory_space.shm.list_all()
    assert len(shm_units) == 1
    assert shm_units[0].content == "Database"
    assert shm_units[0].memory_type == MemoryType.SHM
    print("PASS: Promotion successful.")

    print("\n=== PHASE 9 VERIFICATION COMPLETE ===")

if __name__ == "__main__":
    verify_phase9()
