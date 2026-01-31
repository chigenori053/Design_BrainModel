
import pytest
from design_brain_model.brain_model.phase18 import SoundBox, ExecutionReport

class TestSoundBox:
    """
    Tests for Phase18-3: SoundBox.
    Focuses on B-01 (Isolated Execution) and B-02 (Non-Judgment).
    """

    def test_B01_execution_simulation(self):
        """
        B-01: Verify execution simulation catches basic errors (isolation mock).
        """
        soundbox = SoundBox()
        
        # Case 1: Valid Syntax
        stubs_valid = {"main.py": "x = 1\nprint(x)"}
        report_valid = soundbox.execute(stubs_valid)
        assert report_valid.success is True
        assert len(report_valid.errors) == 0
        assert "Runtime Check: Syntax Valid." in report_valid.logs
        
        # Case 2: Invalid Syntax
        stubs_invalid = {"main.py": "def broken():\n  print('missing indent'"} 
        report_invalid = soundbox.execute(stubs_invalid)
        assert report_invalid.success is False
        assert len(report_invalid.errors) > 0
        assert any("Syntax Error" in e for e in report_invalid.errors)

    def test_B02_non_judgment_reporting(self):
        """
        B-02: Verify report contains only facts, no evaluation words.
        """
        soundbox = SoundBox()
        stubs = {"main.py": "print('test')"}
        report = soundbox.execute(stubs)
        
        # Check against prohibited words in logs
        prohibited = ["good", "bad", "optimal", "should", "better"]
        full_text = " ".join(report.logs + report.errors)
        
        for word in prohibited:
            assert word not in full_text.lower()
            
        # Verify status is binary/flag based
        assert isinstance(report.success, bool)
