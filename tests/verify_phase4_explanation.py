import unittest
import uuid
import re
from hybrid_vm.control_layer.state import (
    DecisionOutcome, ConsensusStatus, EvaluationResult, UtilityVector, Role, RankedCandidate
)
from hybrid_vm.control_layer.explanation_generator import ExplanationGenerator

class TestPhase4Explanation(unittest.TestCase):
    
    def setUp(self):
        self.generator = ExplanationGenerator()
        
    def create_mock_outcome(self, 
                            status: ConsensusStatus, 
                            confidence: float = 0.8, 
                            entropy: float = 0.2, 
                            human_reason: str = None, 
                            lineage: str = None) -> DecisionOutcome:
        
        # Mock Ranked Candidate
        winner = RankedCandidate(
            candidate_id="cand_1",
            content="Use Python",
            final_score=0.9,
            utility_vector_snapshot=UtilityVector(performance=0.9)
        )
        
        # Mock Evaluations
        ev = EvaluationResult(
            evaluator_id="agent1",
            candidates=["cand_1"],
            utility_vector=UtilityVector(performance=0.9),
            confidence=confidence,
            entropy=entropy
        )
        
        return DecisionOutcome(
            resolves_question_id="q1",
            consensus_status=status,
            evaluations=[ev, ev], # Two evals to test aggregation logic
            ranked_candidates=[winner],
            human_reason=human_reason,
            lineage=lineage,
            explanation="" # To be filled
        )

    def test_structure_validity(self):
        """
        6.1 構造整合性検証
        Output must contain specific sections.
        """
        outcome = self.create_mock_outcome(ConsensusStatus.ACCEPT)
        explanation = self.generator.generate(outcome)
        
        self.assertIn("【決定概要】", explanation)
        self.assertIn("【判断根拠】", explanation)
        self.assertIn("【履歴】", explanation)
        self.assertIn("ステータス: ACCEPT", explanation)
        self.assertIn("選択された候補: Use Python", explanation)
        
        # Check aggregation
        self.assertIn("評価数: 2", explanation)

    def test_counterfactual_robustness_status(self):
        """
        6.2 反事実耐性検証 (Status Change)
        ACCEPT vs REVIEW vs ESCALATE
        """
        # Case A: ACCEPT
        exp_accept = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ACCEPT))
        self.assertNotIn("警告", exp_accept)
        
        # Case B: REVIEW (High Entropy)
        exp_review = self.generator.generate(self.create_mock_outcome(ConsensusStatus.REVIEW, entropy=0.8))
        self.assertIn("【不確実性説明】", exp_review)
        self.assertIn("警告: エントロピーが高いため", exp_review)
        
        # Case C: ESCALATE (Low Confidence)
        exp_escalate = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ESCALATE, confidence=0.2))
        self.assertIn("【不確実性説明】", exp_escalate)
        self.assertIn("警告: 確信度が低いため", exp_escalate)

    def test_counterfactual_robustness_human_override(self):
        """
        6.2 反事実耐性検証 (Human Override)
        """
        exp_normal = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ACCEPT))
        exp_human = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ACCEPT, human_reason="User Request"))
        
        self.assertNotIn("【人間介入】", exp_normal)
        self.assertIn("【人間介入】", exp_human)
        self.assertIn("理由: User Request", exp_human)

    def test_counterfactual_robustness_lineage(self):
        """
        6.2 反事実耐性検証 (Re-evaluation)
        """
        exp_new = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ACCEPT, lineage=None))
        exp_re = self.generator.generate(self.create_mock_outcome(ConsensusStatus.ACCEPT, lineage="prev_outcome_id"))
        
        self.assertIn("新規判断", exp_new)
        self.assertIn("再評価 (元: prev_outcome_id)", exp_re)

if __name__ == '__main__':
    unittest.main()
