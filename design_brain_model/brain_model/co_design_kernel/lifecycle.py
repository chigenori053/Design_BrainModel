from contextlib import contextmanager
from typing import Generator
from .state import AgentStateManager
from .types import AgentState

class LifecycleController:
    def __init__(self, state_manager: AgentStateManager):
        self._state_manager = state_manager

    @contextmanager
    def processing_cycle(self) -> Generator[None, None, None]:
        """
        Orchestrates the lifecycle of a single input processing cycle.
        IDLE | DESIGN_REVIEW -> OBSERVING -> (yield) -> RESPONDING -> DESIGN_REVIEW
        """
        # Pre-condition: Must be IDLE or DESIGN_REVIEW
        if self._state_manager.current_state not in [AgentState.IDLE, AgentState.DESIGN_REVIEW]:
             if self._state_manager.current_state == AgentState.INIT:
                 self._state_manager.transition_to(AgentState.IDLE)
             else:
                 raise RuntimeError(f"Cannot start cycle from {self._state_manager.current_state}")

        # 1. OBSERVING
        self._state_manager.transition_to(AgentState.OBSERVING)
        
        try:
            yield # The actual observation and logic happens here
            
        finally:
            # 2. Cleanup / Final Transitions
            if self._state_manager.current_state == AgentState.RESPONDING:
                self._state_manager.transition_to(AgentState.DESIGN_REVIEW)
            elif self._state_manager.current_state == AgentState.DESIGN_REVIEW:
                pass
            else:
                raise RuntimeError(f"Incomplete cycle ended in {self._state_manager.current_state}")

    def startup(self):
        if self._state_manager.current_state == AgentState.INIT:
            self._state_manager.transition_to(AgentState.IDLE)

    def shutdown(self):
        if self._state_manager.current_state != AgentState.TERMINATED:
            # Can only terminate from IDLE
            if self._state_manager.current_state != AgentState.IDLE:
                # Force reset if needed? Or error?
                # For safety, we might want to allow force termination.
                # But state machine says IDLE -> TERMINATED.
                pass 
            
            if self._state_manager.current_state == AgentState.IDLE:
                self._state_manager.transition_to(AgentState.TERMINATED)
