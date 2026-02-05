import pytest

from design_brain_model.brain_model.co_design_kernel.state import AgentStateManager
from design_brain_model.brain_model.co_design_kernel.types import AgentState


class TestPhase20StateMachine:
    def test_waiting_state_removed(self):
        assert "WAITING" not in [state.value for state in AgentState]

    def test_illegal_transitions_raise(self):
        sm = AgentStateManager()
        sm.transition_to(AgentState.IDLE)
        sm.transition_to(AgentState.OBSERVING)

        sm.transition_to(AgentState.PROPOSING)

        with pytest.raises(ValueError):
            sm.transition_to(AgentState.RESPONDING)

        sm.transition_to(AgentState.SIMULATING)
        sm.transition_to(AgentState.RESPONDING)

        with pytest.raises(ValueError):
            sm.transition_to(AgentState.OBSERVING)
