from typing import Dict, Any
from design_brain_model.brain_model.memory.types import DecisionResult, Decision, StoreType

class LanguageDHM:
    """
    Phase 19-1: Language DHM (Text Generation)
    
    Role:
    Generates human-readable text based on Decision, Memory, and DesignEvalDHM results.
    Strictly follows the evaluation signals for tone (completeness/consistency).
    Does NOT alter logic or judgment.
    """

    def generate(
        self,
        decision: DecisionResult,
        memory_state: Dict[str, Any],
        eval_result: Dict[str, Any],
        context: str
    ) -> str:
        
        # Unpack evaluation signals
        completeness = eval_result.get("completeness_score", 0.0)
        consistency = eval_result.get("consistency_score", 0.0)
        missing_slots = eval_result.get("missing_slots", [])
        ambiguity_flags = eval_result.get("ambiguity_flags", [])
        
        store_type = memory_state.get("store_type", "UnknownStore")
        
        # 1. Determine Tone
        # High completeness & consistency -> Assertive
        # Low consistency -> Speculative / Cautious
        # Low completeness -> Warning / Partial
        
        tone_prefix = ""
        tone_suffix = ""
        
        if "low_confidence" in ambiguity_flags or consistency < 0.6:
            tone_prefix = "Based on current analysis, it seems that "
            tone_suffix = " (This requires further verification)."
        elif completeness < 0.8:
            tone_prefix = "Preliminary analysis suggests: "
            if "reason" in missing_slots:
                tone_suffix = " (Note: Detailed reasoning is missing)."
        else:
            tone_prefix = "Confirmed: "
            tone_suffix = "."

        # 2. Construct Core Message based on Decision
        core_message = ""
        
        if decision.label == Decision.ACCEPT:
            core_message = f"the input '{context}' is accepted as valid structure"
        elif decision.label == Decision.REVIEW:
            core_message = f"the input '{context}' requires review"
        elif decision.label == Decision.REJECT:
            core_message = f"the input '{context}' is rejected"
        else:
            core_message = f"the input '{context}' has status {decision.label}"

        # 3. Incorporate Memory Context
        memory_msg = ""
        if store_type == StoreType.QUARANTINE:
            memory_msg = " and has been placed in Quarantine for observation"
        elif store_type == StoreType.CANONICAL:
            memory_msg = " and has been integrated into the Canonical Store"
        elif store_type == StoreType.WORKING:
            memory_msg = " and is currently held in Working Memory"

        # 4. Incorporate Reason
        reason_msg = ""
        if decision.reason:
            reason_msg = f" because {decision.reason}"
        elif "reason" in missing_slots:
            reason_msg = " due to unspecified criteria"

        # 5. Assemble
        # Handle "High Entropy" or specific flags
        if "high_entropy" in ambiguity_flags:
            core_message += " (with high entropy/uncertainty)"

        full_text = f"{tone_prefix}{core_message}{memory_msg}{reason_msg}{tone_suffix}"
        
        # Cleanup double periods or spaces
        full_text = full_text.replace("..", ".").strip()
        
        return full_text
