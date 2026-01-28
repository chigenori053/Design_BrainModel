from typing import Dict, Optional
import uuid
from datetime import datetime
from design_brain_model.hybrid_vm.control_layer.state import (
    EvaluationResult, UtilityVector, Role
)

class HumanOverrideHandler:
    """
    Phase 3: Human Override Handler.
    Converts explicit human decisions into standard EvaluationResults
    that the Consensus Engine can process (usually forcing an outcome).
    """
    
    def create_human_evaluation(self, decision: str, reason: str, candidate_ids: list[str], timestamp: datetime) -> EvaluationResult:
        """
        Creates an EvaluationResult representing a Human Override.
        
        Args:
            decision: "ACCEPT", "REJECT", etc. (Currently mapped to high utility)
            reason: The explanation provided by the human.
            candidate_ids: The candidates this override applies to (usually the winner).
        
        Returns:
            EvaluationResult with Max Confidence and specific Utility.
        """
        
        # Interpret "ACCEPT" as max utility for Phase 3 MVP
        # In a real system, we might differ based on *which* candidate is accepted.
        # Here we assume the input implies "Approve the top candidate" or similar context.
        
        # For MVP: We assume this evaluation supports the candidates provided.
        utility = UtilityVector(
            performance=1.0,
            cost=1.0,
            maintainability=1.0,
            scalability=1.0,
            risk=1.0, # Safe
            evaluated_by=Role.USER
        )
        
        return EvaluationResult(
            evaluator_id="human_supervisor",
            candidates=candidate_ids,
            utility_vector=utility,
            confidence=1.0, # Absolute certainty
            entropy=0.0,    # Zero entropy (no confusion)
            timestamp=timestamp
        )

    def is_human_override(self, input_data: Dict) -> bool:
        """
        Checks if a raw input dict is a Human Override command.
        """
        return input_data.get("type") == "HUMAN_OVERRIDE"
