import pytest
from design_brain_model.brain_model.co_design_kernel.kernel import AgentKernel
from design_brain_model.brain_model.co_design_kernel.types import AgentState, StateMessage, ObservationSummary

class TestAgentKernel:
    
    def test_initialization(self):
        kernel = AgentKernel()
        assert kernel.current_state == AgentState.IDLE

    def test_basic_lifecycle(self):
        kernel = AgentKernel()
        
        # Send message
        input_text = "Hello Agent"
        response = kernel.receive_message(input_text)
        
        # Verify Response
        assert isinstance(response, ObservationSummary)
        
        # Verify State moved to DESIGN_REVIEW
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_shutdown(self):
        kernel = AgentKernel()
        kernel.shutdown()
        assert kernel.current_state == AgentState.TERMINATED

    def test_error_recovery(self):
        kernel = AgentKernel()
        
        # Inject a failure in the observer to simulate a crash during OBSERVING
        # We'll monkeypatch the observer instance attached to the kernel
        def broken_observe(*args, **kwargs):
            raise ValueError("Simulated Sensor Failure")
        
        kernel.observer.observe = broken_observe
        
        # Send message
        response = kernel.receive_message("Crash me")
        
        # Verify Response is Error Message
        assert isinstance(response, StateMessage)
        assert "Simulated Sensor Failure" in response.message
        
        # Verify Agent moved to DESIGN_REVIEW
        assert kernel.current_state == AgentState.DESIGN_REVIEW

    def test_input_truncation_summary(self):
        kernel = AgentKernel()
        long_input = "A" * 100
        response = kernel.receive_message(long_input)
        assert isinstance(response, ObservationSummary)
