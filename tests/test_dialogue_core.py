import pytest
from design_brain_model.brain_model.dialogue.controller import DialogueController
from design_brain_model.brain_model.memory.types import Stability
from design_brain_model.brain_model.dialogue.types import DialoguePhase

def test_dialogue_workflow():
    controller = DialogueController()
    
    # 1. Start dialogue with partial input
    raw_input = """目的: 新しいAIエージェントの構築
範囲: 自然言語処理, コード生成"""
    state = controller.start_dialogue(raw_input)
    
    assert state.semantic_unit.objective == "新しいAIエージェントの構築"
    assert "objective" in state.semantic_unit.confirmed_fields
    assert state.readiness.stability != Stability.STABLE
    assert len(state.open_questions) > 0
    
    # 2. Submit answer for scope_out
    state = controller.submit_answer("scope_out", ["画像認識", "音声合成"])
    assert "scope_out" in state.semantic_unit.confirmed_fields
    
    # 3. Submit answer for success_criteria (should reach STABLE if other required fields are met)
    # Note: In our current simple mock decomposer, only objective and scope_in were picked up.
    # We need to fulfill success_criteria to reach STABLE.
    state = controller.submit_answer("success_criteria", ["10秒以内に応答すること"])
    
    # Check if STABLE is reached
    # Required: objective, scope_in, scope_out, success_criteria
    assert "success_criteria" in state.semantic_unit.confirmed_fields
    assert state.readiness.stability == Stability.STABLE
    assert state.phase == DialoguePhase.STABLE
    assert len(state.open_questions) == 0

def test_blocking_issue():
    controller = DialogueController()
    raw_input = "目的: テスト"
    state = controller.start_dialogue(raw_input)
    
    # scope_in と scope_out に同じものを入れて blocking issue を発生させる
    state = controller.submit_answer("scope_in", ["A", "B"])
    state = controller.submit_answer("scope_out", ["B", "C"])
    
    assert len(state.readiness.blocking_issues) > 0
    assert state.readiness.stability != Stability.STABLE
    # blocking issue がある間は新しい質問が出ない (Sec 7.2)
    assert len(state.open_questions) == 0
