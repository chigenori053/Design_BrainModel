from dataclasses import dataclass
from typing import List
from design_brain_model.brain_model.phase18.soundbox import ExecutionReport
from design_brain_model.brain_model.language_engine.domain import LanguageArticulationValidator

@dataclass
class L2ReconstructionFeedback:
    l2_id: str
    snapshot_id: str
    observations: List[str]
    conflicts_with_l2: List[str]
    reason: str = "L2_RESTRUCTURE_REQUIRED"
    next_action_required: str = "User revision"

class ReconstructionFeedbackGenerator:
    """
    Phase 18-3b: L2 Reconstruction Feedback.
    Handles Class B failures (L2 Mismatch).
    Provides facts only; no proposals.
    """
    
    PROHIBITED_WORDS = LanguageArticulationValidator.PROHIBITED_WORDS

    def generate(self, l2_id: str, snapshot_id: str, report: ExecutionReport) -> L2ReconstructionFeedback:
        
        observations = [log for log in report.logs if "Error" in log or "Failed" in log]
        observations.extend(report.errors)
        conflicts = report.l2_alignment_diff
        
        feedback = L2ReconstructionFeedback(
            l2_id=l2_id,
            snapshot_id=snapshot_id,
            observations=observations,
            conflicts_with_l2=conflicts
        )
        
        self._validate_language(feedback)
        return feedback

    def _validate_language(self, feedback: L2ReconstructionFeedback):
        content = " ".join(feedback.observations) + " ".join(feedback.conflicts_with_l2)
        for word in self.PROHIBITED_WORDS:
            if word in content:
                # In feedback generation, we strictly sanitize or fail.
                # For safety, we'll strip or raise. Here raising to enforce design.
                raise ValueError(f"Feedback contains prohibited vocabulary: {word}")
