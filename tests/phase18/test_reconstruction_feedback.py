
import pytest
from design_brain_model.brain_model.phase18 import ReconstructionFeedbackGenerator, ExecutionReport

class TestReconstructionFeedback:
    """
    Tests for Phase18-3b: Reconstruction Feedback.
    Focuses on D-01 (Correct Escalation) and D-02 (Language Constraints).
    """
    
    def test_D01_escalation_content(self):
        """
        D-01: Verify feedback contains only facts and correct reason code.
        """
        generator = ReconstructionFeedbackGenerator()
        report = ExecutionReport(
            success=False, 
            logs=["Error: Critical failure"], 
            errors=["Logic mismatch"], 
            l2_alignment_diff=["Missing field X defined in L2"]
        )
        
        feedback = generator.generate("L2-Escalate", "snap-esc", report)
        
        assert feedback.l2_id == "L2-Escalate"
        assert feedback.reason == "L2_RESTRUCTURE_REQUIRED"
        assert feedback.next_action_required == "User revision"
        
        # Verify content transfer
        assert "Missing field X defined in L2" in feedback.conflicts_with_l2
        assert "Logic mismatch" in feedback.observations # Assuming logs/errors mapped to observations

    def test_D02_language_constraints(self):
        """
        D-02: Verify prohibited vocabulary causes generation failure.
        """
        generator = ReconstructionFeedbackGenerator()
        
        # Case: Prohibited word in Alignment Diff
        report_bad = ExecutionReport(
            success=False, 
            logs=[], 
            errors=[], 
            l2_alignment_diff=["It is likely a bug"] # "likely" is prohibited
        )
        
        with pytest.raises(ValueError): # Strict validation check
            generator.generate("L2-Bad", "snap-bad", report_bad)

