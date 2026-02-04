import pytest
from design_brain_model.brain_model.evaluation_interface.design_eval_dhm import DesignEvalDHM
from design_brain_model.brain_model.evaluation_interface.language_dhm import LanguageDHM
from design_brain_model.brain_model.memory.types import (
    DecisionResult, Decision, StoreType, MemoryStatus, SemanticRepresentation, OriginContext
)

class TestPhase19LanguageEngine:
    
    @pytest.fixture
    def evaluator(self):
        return DesignEvalDHM()

    @pytest.fixture
    def generator(self):
        return LanguageDHM()

    @pytest.fixture
    def basic_semantic_unit(self):
        return SemanticRepresentation(
            semantic_representation=[0.1, 0.2], # Mock vector
            structure_signature={"type": "test_struct", "constraint": "none"},
            origin_context=OriginContext.TEXT,
            confidence=1.0,
            entropy=0.0
        )

    def test_normal_accept_canonical(self, evaluator, generator, basic_semantic_unit):
        """
        Normal Case: High confidence ACCEPT in Canonical Store.
        Should result in high scores and 'Confirmed' tone.
        """
        decision = DecisionResult(
            label=Decision.ACCEPT,
            confidence=0.95,
            entropy=0.1,
            utility=0.8,
            reason="It meets all criteria."
        )
        memory_state = {"store_type": StoreType.CANONICAL, "status": MemoryStatus.ACTIVE}
        context = "User Input A"

        # 1. Evaluate
        eval_result = evaluator.evaluate(decision, memory_state, basic_semantic_unit)
        
        assert eval_result["completeness_score"] == 1.0
        assert eval_result["consistency_score"] == 1.0
        assert not eval_result["missing_slots"]
        assert not eval_result["ambiguity_flags"]

        # 2. Generate
        text = generator.generate(decision, memory_state, eval_result, context)
        
        assert "Confirmed:" in text
        assert "User Input A" in text
        assert "accepted" in text
        assert "Canonical Store" in text
        assert "It meets all criteria" in text

    def test_low_confidence_accept(self, evaluator, generator, basic_semantic_unit):
        """
        Abnormal Case: Low confidence ACCEPT.
        Should flag 'low_confidence' and use cautious tone.
        """
        decision = DecisionResult(
            label=Decision.ACCEPT,
            confidence=0.4, # Low confidence
            entropy=0.5,
            utility=0.5,
            reason="Looks okay."
        )
        memory_state = {"store_type": StoreType.WORKING, "status": MemoryStatus.ACTIVE}
        context = "Ambiguous Input"

        eval_result = evaluator.evaluate(decision, memory_state, basic_semantic_unit)
        
        assert "low_confidence" in eval_result["ambiguity_flags"]
        assert eval_result["consistency_score"] < 1.0
        
        text = generator.generate(decision, memory_state, eval_result, context)
        
        assert "Based on current analysis" in text or "seems that" in text
        assert "This requires further verification" in text

    def test_missing_reason(self, evaluator, generator, basic_semantic_unit):
        """
        Abnormal Case: Missing reason.
        Should drop completeness score and mention it in text.
        """
        decision = DecisionResult(
            label=Decision.REJECT,
            confidence=0.9,
            entropy=0.1,
            utility=0.0,
            reason="" # Missing
        )
        memory_state = {"store_type": StoreType.WORKING, "status": MemoryStatus.ACTIVE}
        context = "Bad Input"

        eval_result = evaluator.evaluate(decision, memory_state, basic_semantic_unit)
        
        assert "reason" in eval_result["missing_slots"]
        assert eval_result["completeness_score"] < 1.0
        
        text = generator.generate(decision, memory_state, eval_result, context)
        
        assert "unspecified criteria" in text
        assert "Preliminary analysis suggests" in text or "Note: Detailed reasoning is missing" in text

    def test_inconsistent_store(self, evaluator, generator, basic_semantic_unit):
        """
        Abnormal Case: ACCEPT but in Quarantine.
        Should drop consistency score.
        """
        decision = DecisionResult(
            label=Decision.ACCEPT,
            confidence=0.9,
            entropy=0.1,
            utility=0.8,
            reason="Good."
        )
        memory_state = {"store_type": StoreType.QUARANTINE, "status": MemoryStatus.ACTIVE}
        context = "Suspicious Input"

        eval_result = evaluator.evaluate(decision, memory_state, basic_semantic_unit)
        
        assert eval_result["consistency_score"] < 1.0
        
        text = generator.generate(decision, memory_state, eval_result, context)
        # Tone should reflect the inconsistency/caution driven by the score
        assert "Based on current analysis" in text or "seems that" in text
        assert "Quarantine" in text
