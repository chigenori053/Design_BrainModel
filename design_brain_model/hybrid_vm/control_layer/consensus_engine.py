from typing import List, Dict, Optional
import math
from design_brain_model.hybrid_vm.control_layer.state import (
    EvaluationResult, UtilityVector, ConsensusStatus, Policy, Role
)

class ConsensusEngine:
    """
    Phase 3: Consensus Engine.
    Aggregates multiple EvaluationResults and determines the final status (ACCEPT/REVIEW/etc).
    """

    def aggregate(self, evaluations: List[EvaluationResult]) -> UtilityVector:
        """
        Aggregates utility vectors. 
        For MVP: Simple average.
        Future: Weighted average based on evaluator confidence or role.
        """
        if not evaluations:
            return UtilityVector()

        avg_perf, avg_cost, avg_maint, avg_scale, avg_risk = 0.0, 0.0, 0.0, 0.0, 0.0
        count = len(evaluations)

        for ev in evaluations:
            vec = ev.utility_vector
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
            risk=avg_risk / count,
            evaluated_by=Role.SYSTEM # Aggregated by System
        )

    def calculate_collective_entropy(self, evaluations: List[EvaluationResult]) -> float:
        """
        Calculates entropy of the consensus to detect disagreement.
        Simplified for MVP: variance of utility scores? Or just average of individual entropies?
        Spec says: "if entropy > threshold -> REVIEW".
        Let's assume we use the average entropy for now, or if distinct evaluators disagree strongly.
        """
        if not evaluations:
            return 0.0
        
        # Method 1: Average individual entropy (if provided by LLM)
        total_entropy = sum(e.entropy for e in evaluations)
        avg_entropy = total_entropy / len(evaluations)
        
        return avg_entropy

    def calculate_collective_confidence(self, evaluations: List[EvaluationResult]) -> float:
        if not evaluations:
            return 0.0
        
        # Average confidence
        total_conf = sum(e.confidence for e in evaluations)
        return total_conf / len(evaluations)

    def decide(self, evaluations: List[EvaluationResult], policy: Policy) -> ConsensusStatus:
        """
        Determines the ConsensusStatus based on aggregated metrics.
        """
        if not evaluations:
            return ConsensusStatus.REJECT # No evaluations?

        # 1. Check for Human Override (Highest Priority)
        # In a real engine, we might separate this, but here check source.
        # Actually, Human Override acts as a "Veto" or "Force Accept".
        # We assume evaluations might contain a Human one.
        
        # 2. Metrics
        entropy = self.calculate_collective_entropy(evaluations)
        confidence = self.calculate_collective_confidence(evaluations)
        
        # Thresholds (Simulated from Config/Spec)
        # Spec: if entropy > threshold → REVIEW
        # Spec: if confidence < threshold → ESCALATE
        
        ENTROPY_THRESHOLD = 0.6  # High entropy means confusion/disagreement
        CONFIDENCE_THRESHOLD = 0.4 # Low confidence means uncertainty
        
        if entropy > ENTROPY_THRESHOLD:
            return ConsensusStatus.REVIEW
            
        if confidence < CONFIDENCE_THRESHOLD:
            return ConsensusStatus.ESCALATE
            
        return ConsensusStatus.ACCEPT
