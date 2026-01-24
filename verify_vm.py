import sys
import os

# Ensure project root is in path
sys.path.append(os.getcwd())

from hybrid_vm.core import HybridVM
from hybrid_vm.events import UserInputEvent

def test_vm_loop():
    print("Initializing Hybrid VM...")
    vm = HybridVM()
    
    print("Initial State:", vm.get_state_snapshot())
    
    # Simulate User Input
    input_text = "Reflexive: The system must use a database to store user profiles."
    print(f"\nSending User Input: '{input_text}'")
    
    event = UserInputEvent(payload={"content": input_text})
    vm.process_event(event)
    
    # Verify State Update
    state = vm.state
    
    # Check Message
    assert len(state.conversation.history) == 1
    print("\n[Passed] Conversation history updated.")
    
    # Check Semantic Units
    # Mock logic extracts "Database" if "database" is in text
    # Mock logic extracts "must" constraint
    units = state.semantic_units.units
    print(f"Extracted Units: {len(units)}")
    for uid, unit in units.items():
        print(f" - {unit.type}: {unit.content}")
        
    assert len(units) >= 1
    print("\n[Passed] Semantic Units extracted.")

    # --- Test Simulation ---
    from hybrid_vm.events import BaseEvent, EventType
    print("\nRequesting Simulation...")
    sim_event = BaseEvent(type=EventType.SIMULATION_REQUEST, payload={})
    vm.process_event(sim_event)
    
    # Check Simulation State
    assert vm.state.simulation.last_result is not None
    print(f"Simulation Result: {vm.state.simulation.last_result}")
    
    if vm.state.execution_feedback.last_error:
        print(f"Feedback Type: {vm.state.execution_feedback.error_type}")
    
    print("\n[Passed] Simulation executed.")
    
    print("\nVM Verification Successful!")

if __name__ == "__main__":
    test_vm_loop()
