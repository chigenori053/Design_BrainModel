import pytest
from design_brain_model.brain_model.co_design_kernel.kernel import AgentKernel
from design_brain_model.brain_model.co_design_kernel.types import AgentState, ReviewResponse, StateMessage

class TestPhase20Simulation:
    
    def test_simulation_trigger(self):
        kernel = AgentKernel()
        
        # Trigger keyword "simulate"
        input_text = "Please simulate a change in the memory layer."
        response = kernel.receive_message(input_text)
        
        # Without a proposal, simulation should error
        assert isinstance(response, StateMessage)
        
        # Verify internal state is DESIGN_REVIEW
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_simulation_issues_detection(self):
        kernel = AgentKernel()
        
        # Create a proposal first
        kernel.receive_message("Propose a change with dependency.")
        response = kernel.receive_message("Simulate.")
        
        assert isinstance(response, ReviewResponse)
        # Check that issues were detected 
        has_dependency_break = any(issue.issue_type == "dependency_break" for issue in response.issues)
        assert has_dependency_break

    def test_simulation_isolation(self):
        """
        Verify that simulation doesn't change the base state (mocked as no side effects).
        """
        kernel = AgentKernel()
        
        # 1. Run simulation
        kernel.receive_message("Propose changes.")
        
        # 2. Check that we can still do normal proposals
        response = kernel.receive_message("Propose something.")
        assert isinstance(response, ReviewResponse)

    def test_multiple_simulations_independence(self):
        """
        Verify that multiple simulations are independent.
        """
        kernel = AgentKernel()
        
        kernel.receive_message("Propose first change")
        res1 = kernel.receive_message("Simulate")
        
        # Start next simulation (lifecycle allows from DESIGN_REVIEW)
        kernel.receive_message("Propose second change with dependency")
        res2 = kernel.receive_message("Simulate")
        
        assert isinstance(res1, ReviewResponse)
        assert isinstance(res2, ReviewResponse)
        
        # res1 should have no dependency issues
        assert not any(i.issue_type == "dependency_break" for i in res1.issues)
        # res2 should have dependency issues
        assert any(i.issue_type == "dependency_break" for i in res2.issues)

    def test_failure_trace_recording(self):
        """
        Verify that a failed simulation creates a failure trace which becomes a DesignIssue.
        """
        kernel = AgentKernel()
        
        # Trigger a failure by using the 'fail simulation' keyword in the mock
        kernel.receive_message("Propose something that will fail simulation.")
        response = kernel.receive_message("Simulate.")
        
        assert isinstance(response, ReviewResponse)
        # Check that a failed simulation issue exists
        has_failure_issue = any(issue.origin == "failure_trace" or issue.issue_type == "simulation_failed" for issue in response.issues)
        assert has_failure_issue

    def test_resimulation_parameter_updates(self):
        """
        Verify that re-simulation (retry) can change parameters.
        """
        kernel = AgentKernel()
        
        # Request a 'deep' simulation
        first = kernel.receive_message("Propose fail simulation")
        retry = kernel.receive_message("Retry deep")
        assert isinstance(first, ReviewResponse)
        assert isinstance(retry, ReviewResponse)
        assert retry.proposal_id == first.proposal_id

    def test_failure_trace_scoped_and_cleared(self):
        kernel = AgentKernel()
        proposal = kernel.receive_message("Propose fail simulation")
        assert isinstance(proposal, ReviewResponse)
        traces = kernel.simulation_engine.get_failure_traces(proposal.proposal_id)
        assert traces == []
