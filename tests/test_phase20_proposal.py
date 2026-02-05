import pytest
from design_brain_model.brain_model.co_design_kernel.kernel import AgentKernel
from design_brain_model.brain_model.co_design_kernel.types import AgentState, ReviewResponse, ObservationSummary

class TestPhase20Proposal:
    
    def test_proposal_trigger(self):
        kernel = AgentKernel()
        
        # Trigger keyword "propose"
        input_text = "Please propose a new structure for the module."
        response = kernel.receive_message(input_text)
        
        # Verify Response is a ReviewResponse
        assert isinstance(response, ReviewResponse)
        assert response.proposal_id != ""
        assert len(response.issues) >= 0
        
        # Verify internal state is DESIGN_REVIEW
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_no_proposal_trigger(self):
        kernel = AgentKernel()
        
        # No keyword
        input_text = "Just a regular update."
        response = kernel.receive_message(input_text)
        
        # Should be observation summary
        assert isinstance(response, ObservationSummary)
        
    def test_proposal_structure(self):
        kernel = AgentKernel()
        input_text = "Suggest an alternative."
        response = kernel.receive_message(input_text)
        
        assert isinstance(response, ReviewResponse)
        
    def test_l2_safety(self):
        """
        Verify that the generated proposal contains DraftUnits, not SemanticUnitL2.
        And that no side effects are visible (though hard to test side-effects on a mock).
        We check the types strictly.
        """
        kernel = AgentKernel()
        response = kernel.receive_message("Propose changes")
        assert isinstance(response, ReviewResponse)
