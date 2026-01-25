from typing import List, Dict, Optional
import uuid
from datetime import datetime

from hybrid_vm.control_layer.state import (
    DecisionCandidate, UtilityVector, Policy, DecisionOutcome, Role,
    DecisionState, RankedCandidate, EvaluationResult, ConsensusStatus
)
from hybrid_vm.control_layer.consensus_engine import ConsensusEngine
from hybrid_vm.control_layer.explanation_generator import ExplanationGenerator

class Evaluator:
    """
    Interface for any entity that can evaluate a candidate.
    Real implementations would likely use LLMs or simulation results.
    For this MVP, we use a simple deterministic calculator.
    """
    def evaluate(self, candidate: DecisionCandidate, context: Dict = None) -> UtilityVector:
        raise NotImplementedError

class SimpleEvaluator(Evaluator):
    def evaluate(self, candidate: DecisionCandidate, context: Dict = None) -> UtilityVector:
        # Mock logic: deterministic hash-based scoring for reproducibility
        # In a real system, this would analyze the content text.
        seed = len(candidate.content)
        return UtilityVector(
            performance=(seed % 10) / 10.0,
            cost=((seed * 2) % 10) / 10.0,
            maintainability=((seed * 3) % 10) / 10.0,
            scalability=((seed * 4) % 10) / 10.0,
            risk=((seed * 5) % 10) / 10.0,
            evaluated_by=Role.BRAIN
        )

class DecisionPipeline:
    def __init__(self):
        self.evaluator = SimpleEvaluator()
        self.consensus_engine = ConsensusEngine() # Phase 3
        self.explanation_generator = ExplanationGenerator() # Phase 4

    def compute_utility(self, candidate: DecisionCandidate, context: Dict = None) -> UtilityVector:
        """
        Computes the raw utility vector for a candidate.
        """
        if candidate.utility:
            return candidate.utility
        
        utility = self.evaluator.evaluate(candidate, context)
        
        # Phase 2.1: Attribution
        # In a real pipeline, the Evaluator might be the User or Brain. 
        # Here we mock it as the BRAIN since this is the automated pipeline.
        utility.evaluated_by = Role.BRAIN 
        
        candidate.utility = utility # Cache execution
        return utility

    def apply_policy(self, utility: UtilityVector, policy: Policy) -> float:
        """
        Computes a scalar score based on the policy weights.
        """
        score = 0.0
        # Weights should ideally sum to 1.0, but we handle arbitrary weights
        total_weight = sum(policy.weights.values())
        if total_weight == 0:
            return 0.0

        score += utility.performance * policy.weights.get("performance", 0.0)
        score += utility.cost * policy.weights.get("cost", 0.0)
        score += utility.maintainability * policy.weights.get("maintainability", 0.0)
        score += utility.scalability * policy.weights.get("scalability", 0.0)
        score += utility.risk * policy.weights.get("risk", 0.0) 
        
        return score / total_weight

    def rank_candidates(self, candidates: List[DecisionCandidate], policy: Policy) -> List[DecisionCandidate]:
        """
        Ranks candidates by applying the policy to their utility vectors.
        This is essentially a "Single Evaluator" ranking.
        """
        # Ensure all have utility computed
        for cand in candidates:
            if not cand.utility:
                self.compute_utility(cand)
        
        # Sort desc by score
        ranked = sorted(
            candidates, 
            key=lambda c: self.apply_policy(c.utility, policy), 
            reverse=True
        )
        return ranked

    def process_decision(self, question_id: str, candidates: List[DecisionCandidate], policy: Policy, external_evaluations: List[EvaluationResult] = None) -> DecisionOutcome:
        """
        Full pipeline execution with Phase 3 Consensus.
        """
        # 1. Rank candidates (Side effect: computes utility if missing)
        # We still do this to have base utilities on candidates.
        sorted_candidates = self.rank_candidates(candidates, policy)
        
        # 2. Generate EvaluationResult(s)
        # In Phase 3, we treat the "SimpleEvaluator" result as ONE EvaluationResult.
        # We assume it supports the top-ranked candidate strongly? 
        # Or rather, it provides a utility vector that we used for ranking.
        
        # Let's create an EvaluationResult for the top candidate (or all?).
        # The ConsensusEngine aggregates inputs.
        # For this MVP, let's say the SimpleEvaluator provides ONE opinion on the winner.
        
        evaluations = external_evaluations or []
        
        # Create EvaluationResult for each candidate? 
        # No, EvaluationResult usually represents an evaluator's view on the SET or the CHOICES.
        # But our schema says `candidates: List[str]`.
        # Let's say the Evaluator provides vectors for all.
        # But ConsensusEngine.aggregate expects ONE vector per EvaluationResult.
        # So maybe EvaluationResult is "Here is my Utility Vector assessment of the PROPOSAL X".
        # If we have multiple candidates, do we have multiple vectors?
        # Typically consensus is on the "Selected Option".
        
        # Current Logic: We identified a Winner (Sort 0).
        winner = sorted_candidates[0] if sorted_candidates else None
        
        if winner and winner.utility:
             # Wrap the logic as a "Brain Evaluation"
             ev_result = EvaluationResult(
                 evaluator_id="simple_evaluator_v1",
                 candidates=[winner.candidate_id],
                 utility_vector=winner.utility,
                 confidence=0.8, # Mock confidence
                 entropy=0.2     # Mock entropy
             )
             evaluations.append(ev_result)

        # 3. Consensus
        # (If we had other evaluators, e.g. SafetyChecker, we would add them here)
        aggregated = self.consensus_engine.aggregate(evaluations)
        status = self.consensus_engine.decide(evaluations, policy)

        # 4. Create Outcome (Snapshot)
        ranked_snapshots = []
        for cand in sorted_candidates:
            if not cand.utility: continue
            score = self.apply_policy(cand.utility, policy)
            snapshot = RankedCandidate(
                candidate_id=cand.candidate_id,
                content=cand.content,
                final_score=score,
                utility_vector_snapshot=cand.utility.model_copy()
            )
            ranked_snapshots.append(snapshot)

        # Phase 4: Generate Explanation
        # Construct a temporary outcome to pass to generator (or just populate everything then call generate)
        # To avoid circular dependency or partial object, we construct the object first with empty explanation
        # then update it.
        
        policy_id = str(uuid.uuid4())
        
        outcome = DecisionOutcome(
            resolves_question_id=question_id,
            policy_id=policy_id,
            policy_snapshot=policy.weights.copy(),
            
            # Phase 3 Fields
            evaluations=evaluations,
            consensus_status=status,
            lineage=None, # Initial decision
            
            ranked_candidates=ranked_snapshots,
            explanation="" # Placeholder
        )
        
        outcome.explanation = self.explanation_generator.generate(outcome)
        
        return outcome

