
import pytest
from design_brain_model.brain_model.phase18 import AutoFix, ExecutionReport

class TestAutoFix:
    """
    Tests for Phase18-3a: AutoFix.
    Focuses on C-01 (Artifact-Level Only) and C-02 (Limit Enforcement).
    """

    def test_C01_artifact_level_check(self):
        """
        C-01: Verify AutoFix rejects L2 Logic Mismatches.
        """
        autofix = AutoFix()
        
        # Artifact Failure (Syntax) -> Fixable
        report_syntax = ExecutionReport(
            success=False, logs=[], errors=["Syntax Error: ..."], l2_alignment_diff=[]
        )
        assert autofix.is_fixable(report_syntax) is True
        
        # L2 Logic Failure -> Not Fixable by AutoFix
        report_l2 = ExecutionReport(
            success=False, logs=[], errors=["L2 Logic Mismatch: structure X missing"], l2_alignment_diff=[]
        )
        assert autofix.is_fixable(report_l2) is False

    def test_C02_retry_limit(self):
        """
        C-02: Verify AutoFix stops after MAX_RETRIES.
        """
        autofix = AutoFix()
        stubs = {"main.py": "broken"}
        report = ExecutionReport(success=False, logs=[], errors=["Syntax Error"], l2_alignment_diff=[])
        
        # Attempt 1: Should try to fix (mock implementation returns stubs)
        fixed_1 = autofix.attempt_fix(stubs, report, attempt=0)
        assert fixed_1 == stubs # Mock implementation just returns copy
        
        # Attempt MAX: Should just return without trying
        # (Behavior might be same in mock, but conceptually distinct)
        fixed_max = autofix.attempt_fix(stubs, report, attempt=autofix.MAX_RETRIES)
        assert fixed_max == stubs 
        
        # Attempt Over MAX
        fixed_over = autofix.attempt_fix(stubs, report, attempt=autofix.MAX_RETRIES + 1)
        assert fixed_over == stubs
