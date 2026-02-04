import pytest
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi
from design_brain_model.brain_model.evaluation_interface.adapter import InteractionResultDTO, DecisionDTO, MemoryDTO
from design_brain_model.brain_model.evaluation_interface.state_evaluation_engine import EvaluationStatus

class TestPhase19UiApi:
    
    @pytest.fixture
    def api(self):
        return UiApi()

    def test_interaction_flow(self, api):
        """
        Verifies the full interaction loop:
        Input -> VM -> Decision -> Eval -> Language -> DTO
        """
        input_text = "Test Input for Phase 19"
        
        # 1. Run Interaction
        result = api.run_interaction(input_text)
        
        # 2. Check DTO Structure
        assert isinstance(result, InteractionResultDTO)
        assert result.input == input_text
        assert result.response_text is not None
        assert len(result.response_text) > 0
        
        # 3. Check Decision DTO
        assert isinstance(result.decision, DecisionDTO)
        assert result.decision.confidence is not None
        # Mock Evaluator returns specific values, just check existence
        assert result.decision.utility is not None 
        
        # 4. Check Memory DTO
        assert isinstance(result.memory, MemoryDTO)
        # Newly created units in VM start as UNSTABLE (Working Memory)
        # or REVIEW if transitioned. 
        # In _handle_user_input, we create as UNSTABLE.
        # But wait, my manual step in run_interaction doesn't transition the unit state,
        # it just creates a decision ABOUT the unit.
        # So the unit should be UNSTABLE -> Working Memory.
        assert result.memory.store == "WorkingMemory"
        assert result.memory.semantic_id is not None
        
        # 5. Check Design Eval
        assert result.design_eval is not None
        assert result.design_eval.completeness > 0.0

    def test_state_evaluation_integration(self, api):
        """
        Verifies that interactions feed into the StateEvaluationEngine.
        """
        # Run a few interactions
        api.run_interaction("Input 1")
        api.run_interaction("Input 2")
        
        # Get Report
        report = api.get_evaluation_report()
        
        # Check Stats
        assert report["overall_status"] in [EvaluationStatus.OK.value, EvaluationStatus.WARNING.value]
        assert report["decision_stats"]["confidence"]["count"] == 2
        assert report["memory_stats"]["status"] is not None

    def test_read_only_constraint(self, api):
        """
        Verifies that the DTOs are frozen/immutable (to some extent).
        dataclasses with frozen=True should raise error on assignment.
        """
        result = api.run_interaction("Immutable Test")
        
        with pytest.raises(FrozenInstanceError):
            result.decision.confidence = 1.0

# Helper for frozen dataclass check
from dataclasses import FrozenInstanceError
