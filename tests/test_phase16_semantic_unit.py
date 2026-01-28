import pytest
import numpy as np
from design_brain_model.brain_model.memory.types import SemanticRepresentation, OriginContext
from design_brain_model.brain_model.language_engine import LanguageEngine

@pytest.fixture
def engine():
    return LanguageEngine()

def test_create_representations_from_text(engine):
    """
    Tests the successful creation of SemanticRepresentation objects from a Japanese text input.
    This test relies on the logic of `decompose_text`.
    """
    input_text = "目的としていたが、重要ではないかと考えなおした。\n\n最終的には採用されたが、リスクの可能性がある。"
    
    representations = engine.create_representations_from_text(input_text)
    
    # decompose_text splits this into 4 blocks: GOAL, SHIFT, DECISION, RISK
    assert len(representations) == 4

    # --- Verify the first representation (GOAL) ---
    sr_goal = representations[0]
    goal_content = "目的としていたが、"
    
    assert sr_goal.id is not None
    assert sr_goal.origin_context == OriginContext.TEXT
    
    # Check structure signature
    assert sr_goal.structure_signature["type"] == "GOAL"
    assert sr_goal.structure_signature["content"] == goal_content
    
    # Check metrics
    expected_confidence, expected_entropy = engine._calculate_metrics(goal_content)
    assert sr_goal.confidence == pytest.approx(expected_confidence)
    assert sr_goal.entropy == pytest.approx(expected_entropy)
    
    # Check holographic representation (mocked)
    expected_repr = engine._generate_holographic_representation(goal_content)
    assert isinstance(sr_goal.semantic_representation, np.ndarray)
    assert np.array_equal(sr_goal.semantic_representation, expected_repr)

    # --- Verify the second representation (SHIFT) ---
    sr_shift = representations[1]
    shift_content = "重要ではないかと考えなおした。"
    assert sr_shift.structure_signature["type"] == "SHIFT"
    assert sr_shift.structure_signature["content"] == shift_content

    # --- Verify the third representation (DECISION) ---
    sr_decision = representations[2]
    decision_content = "最終的には採用されたが、"
    assert sr_decision.structure_signature["type"] == "DECISION"
    assert sr_decision.structure_signature["content"] == decision_content

    # --- Verify the fourth representation (RISK) ---
    sr_risk = representations[3]
    risk_content = "リスクの可能性がある。"
    assert sr_risk.structure_signature["type"] == "RISK"
    assert sr_risk.structure_signature["content"] == risk_content


def test_empty_or_whitespace_text_returns_empty_list(engine):
    """
    Tests that an empty or whitespace-only string results in an empty list.
    """
    assert engine.create_representations_from_text("") == []
    assert engine.create_representations_from_text("    ") == []
    assert engine.create_representations_from_text("\n \t \n") == []

def test_representation_ids_are_unique(engine):
    """
    Tests that each generated SemanticRepresentation has a unique ID.
    """
    input_text = "これはテストです。\n\nこれもテストです。"
    representations = engine.create_representations_from_text(input_text)
    
    assert len(representations) == 2
    
    # Get all the IDs
    ids = {sr.id for sr in representations}
    
    # The number of unique IDs should be equal to the number of representations
    assert len(ids) == len(representations)
