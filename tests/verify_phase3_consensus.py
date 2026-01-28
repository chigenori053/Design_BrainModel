import unittest
from datetime import datetime, timezone
from design_brain_model.hybrid_vm.control_layer.state import (
    EvaluationResult, UtilityVector, ConsensusStatus, Policy, DecisionCandidate, Role
)
from design_brain_model.hybrid_vm.control_layer.consensus_engine import ConsensusEngine
from design_brain_model.hybrid_vm.control_layer.reevaluation import ReevaluationLoop
from design_brain_model.hybrid_vm.control_layer.decision_pipeline import DecisionPipeline
from design_brain_model.hybrid_vm.control_layer.human_override import HumanOverrideHandler

class TestPhase3Consensus(unittest.TestCase):
    
    def setUp(self):
        self.engine = ConsensusEngine()
        self.pipeline = DecisionPipeline()
        self.reevaluation = ReevaluationLoop(self.engine)
        self.human_handler = HumanOverrideHandler()
        
        self.policy = Policy(name="TestPolicy", weights={"performance": 1.0})
        self.candidate = DecisionCandidate(
            resolves_question_id="q1",
            content="Option A",
            proposed_by=Role.SYSTEM
        )

    def test_consensus_accept(self):
        # Case 1: High Confidence, Low Entropy -> ACCEPT
        ev1 = EvaluationResult(
            evaluator_id="agent1",
            candidates=["c1"],
            utility_vector=UtilityVector(performance=0.9),
            confidence=0.9,
            entropy=0.1
        )
        status = self.engine.decide([ev1], self.policy)
        self.assertEqual(status, ConsensusStatus.ACCEPT)

    def test_consensus_review(self):
        # Case 2: High Entropy -> REVIEW
        ev1 = EvaluationResult(
            evaluator_id="agent1",
            candidates=["c1"],
            utility_vector=UtilityVector(performance=0.9),
            confidence=0.9,
            entropy=0.8 # High entropy
        )
        status = self.engine.decide([ev1], self.policy)
        self.assertEqual(status, ConsensusStatus.REVIEW)

    def test_consensus_escalate(self):
        # Case 3: Low Confidence -> ESCALATE
        ev1 = EvaluationResult(
            evaluator_id="agent1",
            candidates=["c1"],
            utility_vector=UtilityVector(performance=0.5),
            confidence=0.2, # Low confidence
            entropy=0.1
        )
        status = self.engine.decide([ev1], self.policy)
        self.assertEqual(status, ConsensusStatus.ESCALATE)

    def test_human_override_integration(self):
        # Human Override should result in strong evaluation
        human_ev = self.human_handler.create_human_evaluation(
            decision="ACCEPT",
            reason="I say so",
            candidate_ids=[self.candidate.candidate_id],
            timestamp=datetime(1970, 1, 1, tzinfo=timezone.utc)
        )
        
        result = self.pipeline.process_decision(
            question_id="q1",
            candidates=[self.candidate],
            policy=self.policy,
            external_evaluations=[human_ev]
        )
        
        self.assertEqual(result.consensus_status, ConsensusStatus.ACCEPT)
        self.assertIn(human_ev, result.evaluations)

    def test_reevaluation_lineage(self):
        # Initial Decision
        initial_outcome = self.pipeline.process_decision(
            question_id="q1",
            candidates=[self.candidate],
            policy=self.policy
        )
        
        # Re-evaluate
        new_ev = EvaluationResult(
            evaluator_id="reviewer",
            candidates=[self.candidate.candidate_id],
            utility_vector=UtilityVector(performance=0.5),
            confidence=0.9,
            entropy=0.0
        )
        
        new_outcome = self.reevaluation.reevaluate(
            previous_outcome=initial_outcome,
            new_evaluations=[new_ev],
            policy=self.policy
        )
        
        self.assertEqual(new_outcome.lineage, initial_outcome.outcome_id)
        self.assertEqual(new_outcome.consensus_status, ConsensusStatus.ACCEPT)

if __name__ == '__main__':
    unittest.main()
