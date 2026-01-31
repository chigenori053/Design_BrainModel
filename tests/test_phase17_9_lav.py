
import pytest
from unittest.mock import MagicMock
from design_brain_model.brain_model.language_engine.domain import (
    LanguageArticulationValidator, LanguageContext, LanguageReport
)
from design_brain_model.hybrid_vm.control_layer.state import (
    DecisionNode, DecisionNodeStatus, DecisionNodeSnapshot,
    DecisionNodeCandidate, ConfidenceLevel, EntropyLevel
)

class TestPhase17_9_LAV:
    
    @pytest.fixture
    def validator(self):
        return LanguageArticulationValidator()

    @pytest.fixture
    def overridden_context(self):
        node = DecisionNode(
            id="d1",
            status=DecisionNodeStatus.OVERRIDDEN_L2,
            override_target_l2="c2",
            selected_candidate=DecisionNodeCandidate(candidate_id="c2", content="B"),
            confidence=ConfidenceLevel.HIGH,
            entropy=EntropyLevel.LOW
        )
        
        snapshot_before = DecisionNodeSnapshot(
            decision_node_id="d1",
            all_candidates=[
                DecisionNodeCandidate(candidate_id="c1", content="A"),
                DecisionNodeCandidate(candidate_id="c2", content="B")
            ],
            selected_candidate=DecisionNodeCandidate(candidate_id="c1", content="A"),
            confidence=ConfidenceLevel.MID,
            entropy=EntropyLevel.MID,
            system_version="Phase17"
        )
        
        snapshot_after = DecisionNodeSnapshot(
            decision_node_id="d1",
            all_candidates=[
                DecisionNodeCandidate(candidate_id="c1", content="A"),
                DecisionNodeCandidate(candidate_id="c2", content="B")
            ],
            selected_candidate=DecisionNodeCandidate(candidate_id="c2", content="B"),
            confidence=ConfidenceLevel.HIGH,
            entropy=EntropyLevel.LOW,
            system_version="Phase17"
        )
        
        return LanguageContext(
            decision_node=node,
            snapshot_before=snapshot_before,
            snapshot_after=snapshot_after
        )

    def test_lav_success(self, validator, overridden_context):
        """Verify standard success flow meeting all LAV requirements."""
        report = validator.validate(overridden_context)
        
        # LAV-01: Decision State
        assert "OVERRIDDEN_L2" in report.decision_state
        assert "human intervention" in report.decision_state
        
        # LAV-02: L2 Description
        assert "c2" in report.l2_description
        assert "structurally assigned" in report.l2_description
        
        # LAV-03: Rationale
        assert "Human Override" in report.override_rationale
        assert "Automatic evaluation is not the final judgment" in report.override_rationale
        
        # LAV-04: Differences
        assert "selected_candidate" in report.differences["changed"]
        assert "confidence" in report.differences["changed"] # MID -> HIGH
        # In this fixture setup, entropy also changed MID -> LOW
        assert "entropy" in report.differences["changed"] 
        
        # LAV-05: Non-Actions
        assert any("Inference was not executed" in s for s in report.non_actions)
        assert any("Re-evaluation was not executed" in s for s in report.non_actions)

    def test_lav_prohibited_words(self, validator, overridden_context):
        """Verify that prohibited words cause failure."""
        # Monkey patch internal method to inject prohibited word
        original_describe = validator._describe_rationale
        validator._describe_rationale = lambda ctx: "This is likely the best choice."
        
        with pytest.raises(ValueError, match="Prohibited vocabulary detected: likely"):
            validator.validate(overridden_context)
            
        validator._describe_rationale = original_describe

    def test_lav_invalid_state(self, validator):
        """Verify LAV only runs on OVERRIDDEN_L2."""
        node = DecisionNode(id="d2", status=DecisionNodeStatus.REVIEW)
        ctx = LanguageContext(decision_node=node, snapshot_before=None, snapshot_after=None)
        
        with pytest.raises(ValueError, match="LAV can only be executed on OVERRIDDEN_L2"):
            validator.validate(ctx)

    def test_lav_determinism(self, validator, overridden_context):
        """Verify output is identical for same input."""
        report1 = validator.validate(overridden_context)
        report2 = validator.validate(overridden_context)
        assert report1 == report2
