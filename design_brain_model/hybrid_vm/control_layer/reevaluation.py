from typing import List, Optional
import uuid
from datetime import datetime
from design_brain_model.hybrid_vm.control_layer.state import (
    DecisionOutcome, ConsensusStatus, EvaluationResult, Policy
)
from design_brain_model.hybrid_vm.control_layer.consensus_engine import ConsensusEngine

class ReevaluationLoop:
    """
    Phase 3: Re-evaluation Loop.
    Handles the workflow when a decision is in REVIEW status or manually re-triggered.
    """
    def __init__(self, consensus_engine: ConsensusEngine):
        self.consensus_engine = consensus_engine

    def reevaluate(self, 
                   previous_outcome: DecisionOutcome, 
                   new_evaluations: List[EvaluationResult], 
                   policy: Policy) -> DecisionOutcome:
        """
        Performs a re-evaluation of a previous decision.
        
        Args:
            previous_outcome: The decision being re-evaluated.
            new_evaluations: The new set of evaluations (may include old ones or new ones).
            policy: The policy to apply.
            
        Returns:
            A new DecisionOutcome linked to the previous one.
        """
        
        # 1. Aggregate
        aggregated_utility = self.consensus_engine.aggregate(new_evaluations)
        
        # 2. Decide
        status = self.consensus_engine.decide(new_evaluations, policy)
        
        # 3. Create New Outcome (Snapshot)
        # We need to reconstruct RankedCandidates if we want to show full detail,
        # but for this MVP, we might rely on the caller to provide ranked candidates?
        # A proper implementation would call DecisionPipeline.rank_candidates again.
        # But `ReevaluationLoop` might just be a coordinator.
        # Let's assume for now we just return the meta-structure and the Caller (DecisionPipeline) fills the candidates.
        # OR, we assume `DecisionPipeline` uses this class.
        
        # Let's create a partial outcome here, or we need to import DecisionPipeline?
        # Circular dependency risk.
        # Better: ReevaluationLoop just computes the "Consensus Part" and returns it?
        # Or, DecisionPipeline *IS* the main entry point and calls ReevaluationLoop logic?
        
        # According to Architecture: "Control Layer ... Re-evaluation Loop".
        # Let's make this class responsible for linking the lineage.
        
        new_explanation = f"Re-evaluation of {previous_outcome.outcome_id}. Status: {status}."
        
        # Note: In a real implementation, we would re-rank candidates here.
        # For this MVP step, we will return a minimal structure to be enriched by the pipeline.
        
        outcome = DecisionOutcome(
            outcome_id="",
            resolves_question_id=previous_outcome.resolves_question_id,
            policy_id=previous_outcome.policy_id, # Reuse policy ID or new one?
            policy_snapshot=policy.weights.copy(),
            evaluations=new_evaluations,
            consensus_status=status,
            lineage=previous_outcome.outcome_id,
            explanation=new_explanation,
            ranked_candidates=[] # Caller must populate this!
        )
        if not outcome.outcome_id:
            outcome.outcome_id = outcome.compute_deterministic_id()
        return outcome
