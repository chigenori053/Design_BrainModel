import pytest
from design_brain_model.brain_model.dialogue.controller import DialogueController
from design_brain_model.brain_model.memory.types import Stability
from design_brain_model.brain_model.dialogue.types import DialoguePhase, HumanOverrideAction

def test_full_phase_c_workflow():
    controller = DialogueController()
    
    # 1. Start dialogue
    state = controller.start_dialogue("目的: 高速な検索エンジン")
    assert state.phase == DialoguePhase.CLARIFYING
    
    # 2. Complete requirements to reach STABLE
    controller.submit_answer("scope_in", ["全文検索", "インデックス作成"])
    controller.submit_answer("scope_out", ["クローリング", "UI表示"])
    state = controller.submit_answer("success_criteria", ["検索レイテンシ 50ms以下"])
    
    # Check if we are in CANDIDATES_READY phase
    assert state.semantic_unit.stability == Stability.STABLE
    assert state.phase == DialoguePhase.CANDIDATES_READY
    assert len(state.candidates) >= 2
    
    # 3. Human Override - Accept a candidate
    selected_candidate = state.candidates[0]
    state = controller.human_override(
        action=HumanOverrideAction.ACCEPT, 
        candidate_id=selected_candidate.id,
        reason="Looks solid."
    )
    
    assert state.phase == DialoguePhase.READONLY
    assert len(controller._override_logs) == 1
    assert controller._override_logs[0].action == HumanOverrideAction.ACCEPT

def test_divergence_and_resync():
    controller = DialogueController()
    controller.start_dialogue("目的: テスト")
    controller.submit_answer("scope_in", ["A"])
    controller.submit_answer("scope_out", ["B"])
    state = controller.submit_answer("success_criteria", ["C"])
    
    assert state.phase == DialoguePhase.CANDIDATES_READY
    
    # Divergence: User wants to change the objective
    state = controller.submit_divergence("目的: まったく別の新しい目的")
    
    # Should move back to CLARIFYING or STABLE depending on the new input
    # In our current mock, it might stay STABLE if all fields are still "confirmed"
    # But Divergence is meant to trigger re-evaluation.
    assert state.semantic_unit.objective == "まったく別の新しい目的"
