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
        IDLE | DESIGN_REVIEW -> OBSERVING -> (yield) -> ...
        APPLY_* -> (yield) -> ...
        """
        # Pre-condition
        current = self._state_manager.current_state
        is_stable = current in [AgentState.IDLE, AgentState.DESIGN_REVIEW] or current.startswith("APPLY_")
        
        if not is_stable:
             if current == AgentState.INIT:
                 self._state_manager.transition_to(AgentState.IDLE)
             else:
                 raise RuntimeError(f"Cannot start cycle from {current}")

        # 1. OBSERVING (skip for APPLY_* flows)
        if current in [AgentState.IDLE, AgentState.DESIGN_REVIEW]:
            self._state_manager.transition_to(AgentState.OBSERVING)
        
        try:
            yield # The actual observation and logic happens here
            
        finally:
            # 2. Cleanup / Final Transitions
            current_now = self._state_manager.current_state
            
            try:
                if current_now == AgentState.RESPONDING:
                    self._state_manager.transition_to(AgentState.DESIGN_REVIEW)
                elif current_now == AgentState.DESIGN_REVIEW or current_now.startswith("APPLY_"):
                    pass 
                elif current_now == AgentState.OBSERVING:
                    raise RuntimeError("Cycle ended in OBSERVING without advancing state.")
            except:
                pass

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
