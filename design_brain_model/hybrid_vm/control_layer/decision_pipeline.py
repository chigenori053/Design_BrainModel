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

    def process_decision(self, question_id: str, candidates: List[DecisionCandidate], policy: Optional[Policy], external_evaluations: List[EvaluationResult] = None) -> DecisionOutcome:
        """
        Full pipeline execution with Phase 3 Consensus.
        """
        # 1. Rank candidates (Side effect: computes utility if missing)
        # Handle Policy None: Use default if missing, though ideally shouldn't happen in normal flow.
        # For Override flow, we might not care about ranking as much as the Override Evaluation.
        
        safe_policy = policy or Policy(name="default", weights={"performance": 1.0})
        
        sorted_candidates = self.rank_candidates(candidates, safe_policy)
        
        # 2. Generate EvaluationResult(s)
        evaluations = external_evaluations or []
        
        # If we have candidates, we try to evaluate them automatically unless overridden strictly
        if sorted_candidates:
            winner = sorted_candidates[0]
            if winner.utility:
                 ev_result = EvaluationResult(
                     evaluator_id="simple_evaluator_v1",
                     candidates=[winner.candidate_id],
                     utility_vector=winner.utility,
                     confidence=0.8,
                     entropy=0.2
                 )
                 evaluations.append(ev_result)

        # 3. Consensus
        # With just Human Override in evaluations, this should yield Reached/HumanOverride logic
        status = self.consensus_engine.decide(evaluations, safe_policy)

        # 4. Create Outcome (Snapshot)
        ranked_snapshots = []
        for cand in sorted_candidates:
            if not cand.utility: continue
            score = self.apply_policy(cand.utility, safe_policy)
            snapshot = RankedCandidate(
                candidate_id=cand.candidate_id,
                content=cand.content,
                final_score=score,
                utility_vector_snapshot=cand.utility.model_copy()
            )
            ranked_snapshots.append(snapshot)
        
        # Phase 4 Extension: Check for Human Evaluator and extract reason
        human_reason = None
        for ev in evaluations:
            # Assuming Role.USER or specific ID for human
            # For Phase 7, we check for 1.0 confidence or specific ID convention?
            # Or better, check the attribute `evaluated_by` if we exposed it on EvaluationResult more clearly.
            # But earlier in Phase 3 we defined EvaluationResult.utility_vector.evaluated_by.
            if ev.utility_vector.evaluated_by == Role.USER:
                 # We need a way to pass the text reason. 
                 # Currently EvaluationResult doesn't have a specific "reason" text field besides utility logs?
                 # Ah, we need to pass the text. 
                 # Let's assume HumanOverrideHandler puts the reason somewhere?
                 # Phase 7 spec says: process_human_override(decision, reason, ...)
                 # But EvaluationResult is struct.
                 # Let's assume we pass it? 
                 # Actually, `ConsensusEngine.decide` might determine consensus status, 
                 # but we need to populate `human_reason` on Outcome.
                 # Hack/Solution for now: If status is manually set or implied human, we might need to carry that info differently?
                 # OR, we might just set it if we know it came from override.
                 pass

        # If external_evaluations contained human input, we might want to capture the reason.
        # But strictly speaking, process_decision signature doesn't take 'reason'.
        # However, HybridVM.process_human_override DID pass 'reason' to `HumanOverrideHandler.create_human_evaluation`.
        # Where does that reason go? 
        # Checking HumanOverrideHandler (implied existence) ... likely it returns an EvaluationResult.
        # If EvaluationResult doesn't have 'reason_text' field, we might lose it.
        # Let's check `state.py`.
        # EvaluationResult has: evaluator_id, candidates, utility_vector, confidence, entropy, timestamp.
        # It does NOT have text reason.
        
        # Critical Fix for data preservation:
        # We can attach the reason to the Outcome AFTER process_decision returns in HybridVM.process_human_override.
        # OR, we assume `process_decision` is purely mechanical utility calculation.
        
        policy_id = str(uuid.uuid4())
        
        outcome = DecisionOutcome(
            resolves_question_id=question_id,
            policy_id=policy_id,
            policy_snapshot=safe_policy.weights.copy(),
            
            # Phase 3 Fields
            evaluations=evaluations,
            consensus_status=status,
            lineage=None, 
            
            ranked_candidates=ranked_snapshots,
            explanation="" 
        )
        
        outcome.explanation = self.explanation_generator.generate(outcome)
        
        return outcome

