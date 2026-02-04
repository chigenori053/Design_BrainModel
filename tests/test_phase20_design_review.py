import pytest
from design_brain_model.brain_model.co_design_kernel.kernel import AgentKernel
from design_brain_model.brain_model.co_design_kernel.types import AgentState, ReviewResponse, DesignIssue, StateMessage

class TestPhase20DesignReview:
    
    def test_design_review_transition(self):
        kernel = AgentKernel()
        
        # Trigger keyword "simulate" which now leads to ReviewResponse
        kernel.receive_message("Propose a new design.")
        response = kernel.receive_message("Simulate.")
        
        # Verify Response is ReviewResponse
        assert isinstance(response, ReviewResponse)
        assert len(response.issues) > 0
        assert isinstance(response.issues[0], DesignIssue)
        
        # Verify State stays in DESIGN_REVIEW (as per lifecycle logic)
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_design_review_issue_factual_nature(self):
        """
        Verify that DesignIssues are factual and do not contain recommendations.
        """
        kernel = AgentKernel()
        kernel.receive_message("Propose with dependency.")
        response = kernel.receive_message("Simulate.")
        
        for issue in response.issues:
            desc = issue.description.lower()
            # Factual keywords allowed
            factual_keywords = ["count", "dependency", "uncertainty", "runtime", "performance", "human", "token", "draft"]
            assert any(k in desc for k in factual_keywords)
            # Actionable/Evaluative keywords forbidden
            for bad_k in ["should", "must", "fix", "improve", "recommend", "better"]:
                assert bad_k not in desc

    def test_design_review_max_issues(self):
        """
        Verify the limit of 5 issues per review.
        """
        kernel = AgentKernel()
        # Mocking to generate many issues would be complex, 
        # but our current mock adds ambiguity + 3 uncertainties.
        # Let's verify it doesn't exceed 5.
        kernel.receive_message("Propose many things.")
        response = kernel.receive_message("Simulate.")
        assert len(response.issues) <= 5

    def test_design_review_persistence_and_next_turn(self):
        """
        Verify we can start a new turn from DESIGN_REVIEW.
        """
        kernel = AgentKernel()
        kernel.receive_message("Propose.")
        kernel.receive_message("Simulate.")
        assert kernel.current_state == AgentState.DESIGN_REVIEW
        
        # Next turn: just a regular message
        response = kernel.receive_message("Thank you.")
        assert isinstance(response, StateMessage)
        assert kernel.current_state == AgentState.DESIGN_REVIEW
