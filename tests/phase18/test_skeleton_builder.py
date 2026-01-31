
import pytest
from design_brain_model.brain_model.phase18 import CodeSkeletonBuilder
from design_brain_model.brain_model.language_engine.domain import LanguageReport

class TestSkeletonBuilder:
    """
    Tests for Phase18-1: CodeSkeletonBuilder.
    Focuses on A-01 (L2 Origin) and A-02 (Responsibility Non-Expansion).
    """

    @pytest.fixture
    def mock_language_report(self):
        return LanguageReport(
            summary="L2 Summary",
            decision_state="OVERRIDDEN_L2",
            l2_description="Desc",
            override_rationale="Reason",
            differences={},
            non_actions=[],
            scope_limits=[]
        )

    def test_A01_l2_origin_guarantee(self, mock_language_report):
        """
        A-01: Verify that generated skeleton is derived ONLY from L2 structure.
        """
        l2_unit = {
            "candidate_id": "L2-TEST-A01",
            "content": "src/domain/entity API", # Structural hint
            "kind": "DECISION"
        }
        
        builder = CodeSkeletonBuilder()
        skeleton = builder.build(l2_unit, "snap-101", mock_language_report)
        
        # Verify Provenance
        assert skeleton.provenance.l2_id == "L2-TEST-A01"
        assert skeleton.provenance.snapshot_id == "snap-101"
        
        # Verify Structure matches L2 content hint
        assert skeleton.directory.paths == ["src/domain/entity"]
        
        # Verify API extracted (mock logic based on keyword presence)
        assert len(skeleton.api.endpoints) > 0

    def test_A02_responsibility_non_expansion(self, mock_language_report):
        """
        A-02: Verify that no external logic or new modules are invented.
        """
        l2_unit = {
            "candidate_id": "L2-TEST-A02",
            "content": "src/simple_module", # No API keyword
            "kind": "DECISION"
        }
        
        builder = CodeSkeletonBuilder()
        skeleton = builder.build(l2_unit, "snap-102", mock_language_report)
        
        # Should contain directory path
        assert skeleton.directory.paths == ["src/simple_module"]
        
        # Should NOT invent API endpoints if not implied by structure
        assert len(skeleton.api.endpoints) == 0
        
        # Should basic types only
        assert "L2Entity" in skeleton.types.types
        # Ensure no random extra types
        assert len(skeleton.types.types) == 1
