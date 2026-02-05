import pytest
from design_brain_model.brain_model.co_design_kernel.kernel import AgentKernel
from design_brain_model.brain_model.co_design_kernel.types import AgentState, StateMessage, ReviewResponse

class TestPhase21Apply:

    def test_apply_flow_blindness(self):
        """
        Verify that Apply flow works and blocks search.
        """
        kernel = AgentKernel()
        print(f"\nInitial State: {kernel.current_state}")
        
        # 1. Generate a proposal to reach DESIGN_REVIEW
        kernel.receive_message("Propose something")
        print(f"State after proposal: {kernel.current_state}")
        assert kernel.current_state == AgentState.DESIGN_REVIEW
        
        # 2. Trigger APPLY flow
        response = kernel.receive_message("apply this proposal")
        print(f"State after apply: {kernel.current_state}")
        assert kernel.current_state == AgentState.APPLY_CONFIRM
        assert "L2Patch" in response.message
        
        # 3. Attempt SEARCH during APPLY (Should be blocked)
        search_response = kernel.receive_message("search for external help")
        assert isinstance(search_response, StateMessage)
        assert "prohibited" in search_response.message.lower()
        # State should remain in APPLY_CONFIRM
        assert kernel.current_state == AgentState.APPLY_CONFIRM 
        
        # 4. Confirm execution
        done_response = kernel.receive_message("confirm apply")
        # Final loop back to DESIGN_REVIEW
        assert kernel.current_state == AgentState.DESIGN_REVIEW
        assert "COMPLETED" in done_response.message

    def test_apply_intent_validation(self):
        """
        Verify that apply requires an active proposal.
        """
        kernel = AgentKernel()
        # Initial is IDLE
        assert kernel.current_state == AgentState.IDLE
        # No proposal yet
        response = kernel.receive_message("apply now")
        assert isinstance(response, StateMessage)
        assert "DESIGN_REVIEW" in response.message
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_search_isolation_after_apply(self):
        """
        Verify search is allowed again after apply is done.
        """
        kernel = AgentKernel()
        kernel.receive_message("Propose something")
        kernel.receive_message("apply")
        kernel.receive_message("confirm")
        assert kernel.current_state == AgentState.DESIGN_REVIEW
        
        # Now search should be allowed
        response = kernel.receive_message("search for new patterns")
        assert "Artifact" in response.summary

    def test_search_is_blocked_during_apply_without_invoking_agent(self, monkeypatch):
        """
        Verify that SearchAgent is not invoked during APPLY state.
        """
        kernel = AgentKernel()
        kernel.receive_message("Propose something")
        kernel.receive_message("apply")

        called = {"count": 0}

        def fake_execute_search(*args, **kwargs):
            called["count"] += 1
            raise AssertionError("SearchAgent should not be called during apply.")

        monkeypatch.setattr(kernel.search_agent, "execute_search", fake_execute_search)
        response = kernel.receive_message("search for external help")
        assert isinstance(response, StateMessage)
        assert "prohibited" in response.message.lower()
        assert called["count"] == 0
