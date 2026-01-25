from typing import List, Dict, Optional
import uuid
from datetime import datetime

from hybrid_vm.control_layer.state import (
    DecisionCandidate, UtilityVector, Policy, DecisionOutcome, Role,
    DecisionState, RankedCandidate
)

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
            risk=((seed * 5) % 10) / 10.0
        )

class DecisionPipeline:
    def __init__(self):
        self.evaluator = SimpleEvaluator()

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
        score += utility.risk * policy.weights.get("risk", 0.0) # Usually risk is negative, but here we treat it as 0-1 score where 1 is good (low risk)? Or 1 is high risk?
        # Specification says "risk: 0.1" in policy. 
        # Usually risk is a penalty. Let's assume for this Phase 2 MVP that 
        # UtilityVector dimensions are "Goodness" (1.0 is best). 
        # So "Risk" dimension should be "Safety" or "LowRisk". 
        # PROPOSAL: Let's assume the UtilityVector values are all "benefit" oriented for now.
        # If Risk is "High Risk", then the policy weight should be negative? 
        # Let's keep it simple: UtilityVector.risk means "Safety Score" (1.0 = Safe, 0.0 = Risky).
        
        return score / total_weight

    def rank_candidates(self, candidates: List[DecisionCandidate], policy: Policy) -> List[DecisionCandidate]:
        """
        Ranks candidates by applying the policy to their utility vectors.
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

    def aggregate_opinions(self, evaluations: List[Dict[str, UtilityVector]]) -> UtilityVector:
        """
        Consensus Model: Variable implementations.
        MVP: Average the vectors.
        evaluations: List of {"role": Role, "vector": UtilityVector}
        """
        if not evaluations:
            return UtilityVector() # Zeros

        avg_perf, avg_cost, avg_maint, avg_scale, avg_risk = 0.0, 0.0, 0.0, 0.0, 0.0
        count = len(evaluations)

        for admission in evaluations:
            vec = admission["vector"]
            avg_perf += vec.performance
            avg_cost += vec.cost
            avg_maint += vec.maintainability
            avg_scale += vec.scalability
            avg_risk += vec.risk

        return UtilityVector(
            performance=avg_perf / count,
            cost=avg_cost / count,
            maintainability=avg_maint / count,
            scalability=avg_scale / count,
            risk=avg_risk / count
        )

    def process_decision(self, question_id: str, candidates: List[DecisionCandidate], policy: Policy) -> DecisionOutcome:
        """
        Full pipeline execution with Phase 2.1 Traceability.
        """
        # 1. Rank candidates (Side effect: computes utility if missing)
        # Note: rank_candidates returns List[DecisionCandidate]
        sorted_candidates = self.rank_candidates(candidates, policy)
        
        # 2. Convert to RankedCandidate (Snapshotting)
        ranked_snapshots = []
        for cand in sorted_candidates:
            # Ensure utility is set (rank_candidates guarantees this, but type check safe)
            if not cand.utility:
                 continue
                 
            # Apply Policy Score again for the record (or we could return it from rank_candidates)
            score = self.apply_policy(cand.utility, policy)
            
            # Phase 2.1: Create Snapshot
            snapshot = RankedCandidate(
                candidate_id=cand.candidate_id,
                content=cand.content,
                final_score=score,
                utility_vector_snapshot=cand.utility.model_copy() # Deep copy utility
            )
            ranked_snapshots.append(snapshot)

        # Explain the winner
        winner = ranked_snapshots[0] if ranked_snapshots else None
        explanation = f"Selected candidate {winner.candidate_id} based on policy '{policy.name}'." if winner else "No candidates available."
        
        # Phase 2.1: Policy Snapshot
        policy_id = str(uuid.uuid4()) # In real system, Policy would have an ID.
        
        return DecisionOutcome(
            resolves_question_id=question_id,
            policy_id=policy_id,
            policy_snapshot=policy.weights.copy(), # Snapshot weights
            ranked_candidates=ranked_snapshots,
            explanation=explanation
        )
