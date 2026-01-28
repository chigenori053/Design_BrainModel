import sys
import os
import uuid
# Ensure we can import modules from PWD
sys.path.append(os.getcwd())

from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.control_layer.state import VMState, DecisionCandidate, Policy, Role, SemanticUnit, SemanticUnitKind, SemanticUnitStatus

def test_phase_2_decision():
    print("=== Starting Phase 2 Decision Verification ===")
    vm = HybridVM.create()
    
    # Setup Context: A Question Unit
    question_unit = SemanticUnit(
        kind=SemanticUnitKind.QUESTION,
        content="Which database sharding strategy should we use?"
    )
    state = VMState.model_validate(vm.get_state_snapshot())
    state.semantic_units.units[question_unit.id] = question_unit
    vm = HybridVM.from_snapshot(state.model_dump(), vm_id=vm.vm_id)
    
    # 1. Create Candidates
    # Candidate A: High Performance, High Cost
    candidate_a = DecisionCandidate(
        resolves_question_id=question_unit.id,
        content="Use Hash Sharding (High Perf, Complex)",
        proposed_by=Role.BRAIN
    )
    # Mocking utility for deterministic test (usually computed by pipeline)
    # SimpleEvaluator uses len(content) hash.
    # "Use Hash Sharding (High Perf, Complex)" len = 38
    # 38 % 10 = 8 (Perf: 0.8)
    # (38*2)%10 = 6 (Cost: 0.6) <-- Wait, let's let the pipeline compute it first to test that.
    
    candidate_b = DecisionCandidate(
        resolves_question_id=question_unit.id,
        content="Use Range Sharding (Easy, Scale risk)",
        proposed_by=Role.USER
    )
    
    candidates = [candidate_a, candidate_b]
    
    # 2. Define Policies
    perf_policy = Policy(
        name="Performance First",
        weights={"performance": 0.8, "cost": 0.1, "risk": 0.1, "maintainability": 0.0, "scalability": 0.0}
    )
    
    cost_policy = Policy(
        name="Cost Saver",
        weights={"performance": 0.1, "cost": 0.8, "risk": 0.1, "maintainability": 0.0, "scalability": 0.0}
    )
    
    print("\n--- Test Case 1: Evaluate with Performance Policy ---")
    vm.evaluate_decision(question_unit.id, candidates, perf_policy)
    
    # Check Outcome
    state = VMState.model_validate(vm.get_state_snapshot())
    outcome_1 = state.decision_state.outcomes[-1]
    winner_1 = outcome_1.ranked_candidates[0]
    print(f"Winner 1: {winner_1.content}")
    # Phase 2.1: Use utility_vector_snapshot
    print(f"Scores 1: {[ (c.content, c.utility_vector_snapshot) for c in outcome_1.ranked_candidates ]}")
    
    # Phase 2.1 Verification: Traceability
    print("Verifying Traceability...")
    assert outcome_1.policy_snapshot == perf_policy.weights
    assert outcome_1.policy_id is not None
    assert winner_1.utility_vector_snapshot.evaluated_by == Role.BRAIN
    print("PASS: Traceability fields present and correct.")

    print("\n--- Test Case 2: Evaluate with Cost Policy ---")
    # Reset utilities? No, they are cached in candidate objects. 
    # But policies weight them differently.
    vm.evaluate_decision(question_unit.id, candidates, cost_policy)
    
    state = VMState.model_validate(vm.get_state_snapshot())
    outcome_2 = state.decision_state.outcomes[-1]
    winner_2 = outcome_2.ranked_candidates[0]
    print(f"Winner 2: {winner_2.content}")
    
    # Phase 2.1: Determinism Check
    print("\n--- Test Case 3: Verify Determinism ---")
    vm.evaluate_decision(question_unit.id, candidates, cost_policy)
    state = VMState.model_validate(vm.get_state_snapshot())
    outcome_3 = state.decision_state.outcomes[-1]
    
    # Check if Outcome 2 and Outcome 3 are identical in content (ignoring IDs/timestamps)
    assert outcome_2.outcome_id != outcome_3.outcome_id
    assert outcome_2.ranked_candidates == outcome_3.ranked_candidates
    assert outcome_2.explanation == outcome_3.explanation
    print("PASS: Same input produced identical ranked output.")

    # 3. Verify Non-Destructive Nature (State Safety)
    print("\n--- Test Case 4: Verify State Safety ---")
    # The Question Unit should still be UNSTABLE (or whatever it started as), NOT changed by decision
    state = VMState.model_validate(vm.get_state_snapshot())
    current_q_unit = state.semantic_units.units[question_unit.id]
    if current_q_unit.status == SemanticUnitStatus.UNSTABLE:
        print("PASS: Question Unit status remains UNSTABLE.")
    else:
        print(f"FAIL: Question Unit status changed to {current_q_unit.status}")

    # Check that no new SemanticUnits were created (candidates are NOT semantic units in this design, they are distinct)
    state = VMState.model_validate(vm.get_state_snapshot())
    if len(state.semantic_units.units) == 1:
        print("PASS: No new SemanticUnits created (DecisionCandidate != SemanticUnit).")
    else:
        print(f"FAIL: Logic created new SemanticUnits: {len(state.semantic_units.units)}")

    print("\n=== Verification Complete ===")

if __name__ == "__main__":
    test_phase_2_decision()
