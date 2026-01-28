import sys
import os

# Add project root to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..', 'design_brain_model')))

from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.control_layer.state import ConsensusStatus

def test_human_override():
    print("=== Testing Human Override Logic ===")
    
    vm = HybridVM.create()
    
    # 1. Simulate Human Override
    # Scenario: Human forces REJECT for a decision
    print("1. Injecting Human Override (Decision: REJECT, Reason: 'Security Risk')...")
    
    outcome = vm.process_human_override(
        decision="REJECT",
        reason="Security Risk detected by Admin",
        candidate_ids=["cand-001"]
    )
    
    # 2. Assertions
    print(f"Outcome Status: {outcome.consensus_status}")
    print(f"Human Reason: {outcome.human_reason}")
    print(f"Explanation: {outcome.explanation}")
    
    assert outcome.consensus_status == ConsensusStatus.REJECT, f"Status should be REJECT, got {outcome.consensus_status}"
    assert "人間介入" in outcome.explanation or "Human Override" in outcome.explanation, "Explanation should mention Human Override/Intervention"
    assert outcome.human_reason == "Security Risk detected by Admin", "Reason should be preserved"
    
    # 3. Verify Event Log
    last_event = vm.event_log[-1]
    print(f"Last Event: {last_event.type}")
    assert last_event.type.value == "decision_outcome_generated"
    
    print("=== Test Passed ===")

if __name__ == "__main__":
    test_human_override()
