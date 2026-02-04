from typing import Dict, Any, List, Optional
from design_brain_model.brain_model.memory.types import DecisionResult, Decision, StoreType, MemoryStatus, SemanticRepresentation

class DesignEvalDHM:
    """
    Phase 19-1: Design Evaluation DHM (Minimal Mathematical Representation)
    
    Role:
    Evaluates the soundness of decision, memory state, and semantic unit structure
    BEFORE text generation.
    It does NOT generate text and does NOT alter the decision.
    It returns scores and flags to guide the tone of LanguageDHM.
    """

    def evaluate(
        self,
        decision: DecisionResult,
        memory_state: Dict[str, Any],  # Expecting {'store_type': StoreType, 'status': MemoryStatus}
        semantic_unit: SemanticRepresentation
    ) -> Dict[str, Any]:
        
        missing_slots = []
        ambiguity_flags = []
        
        # 1. Check Completeness
        # Check if reason is provided
        if not decision.reason or len(decision.reason.strip()) == 0:
            missing_slots.append("reason")
            
        # Check if semantic unit has structure (simplified check)
        if not semantic_unit.structure_signature:
             # In a real scenario, we might look for specific keys like 'constraint'
             # For now, if empty, it's incomplete
             missing_slots.append("structure_signature")

        # 2. Check Consistency & Ambiguity
        # Low confidence check
        if decision.confidence < 0.6: # Threshold for "low_confidence" flag
            ambiguity_flags.append("low_confidence")
            
        # Consistency between Decision and Memory
        store_type = memory_state.get("store_type")
        is_consistent = True
        
        if decision.label == Decision.ACCEPT and store_type == StoreType.QUARANTINE:
            # Accepted items shouldn't usually be in Quarantine immediately (or requires explanation)
            is_consistent = False
            
        if decision.label == Decision.REJECT and store_type == StoreType.CANONICAL:
            # Rejected items shouldn't be in Canonical
            is_consistent = False

        # 3. Calculate Scores (Simple Heuristics - No complex math)
        
        # Completeness Score: 1.0 starts, penalize for missing slots
        completeness_score = 1.0
        if "reason" in missing_slots:
            completeness_score -= 0.5
        if "structure_signature" in missing_slots:
            completeness_score -= 0.3
        completeness_score = max(0.0, completeness_score)
        
        # Consistency Score: 1.0 starts, penalize for inconsistency or low confidence
        consistency_score = 1.0
        if not is_consistent:
            consistency_score -= 0.5
        if "low_confidence" in ambiguity_flags:
            consistency_score -= 0.2
            
        # High entropy also reduces consistency/certainty
        if decision.entropy > 0.8:
            ambiguity_flags.append("high_entropy")
            consistency_score -= 0.2
            
        consistency_score = max(0.0, consistency_score)

        return {
            "completeness_score": completeness_score,
            "consistency_score": consistency_score,
            "missing_slots": missing_slots,
            "ambiguity_flags": ambiguity_flags
        }
