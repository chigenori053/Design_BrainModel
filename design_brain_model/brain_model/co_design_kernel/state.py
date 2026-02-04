from .types import AgentState

class AgentStateManager:
    def __init__(self):
        self._current_state = AgentState.INIT

    @property
    def current_state(self) -> AgentState:
        return self._current_state

    def transition_to(self, target_state: AgentState) -> None:
        """
        Validates and executes state transition.
        Raises ValueError if transition is illegal.
        """
        allowed = False
        
        if self._current_state == AgentState.INIT:
            if target_state == AgentState.IDLE:
                allowed = True

        elif self._current_state == AgentState.IDLE:
            if target_state in [AgentState.OBSERVING, AgentState.TERMINATED]:
                allowed = True

        elif self._current_state == AgentState.OBSERVING:
            if target_state == AgentState.PROPOSING:
                allowed = True

        elif self._current_state == AgentState.PROPOSING:
             if target_state == AgentState.SIMULATING:
                allowed = True

        elif self._current_state == AgentState.SIMULATING:
             if target_state == AgentState.RESPONDING:
                allowed = True

        elif self._current_state == AgentState.RESPONDING:
            if target_state == AgentState.DESIGN_REVIEW:
                allowed = True

        elif self._current_state == AgentState.DESIGN_REVIEW:
            if target_state == AgentState.OBSERVING:
                allowed = True
        
        elif self._current_state == AgentState.TERMINATED:
            # No exit from terminated
            allowed = False

        if not allowed:
            raise ValueError(f"Illegal transition: {self._current_state} -> {target_state}")
        
        self._current_state = target_state

    def is_terminal(self) -> bool:
        return self._current_state == AgentState.TERMINATED
