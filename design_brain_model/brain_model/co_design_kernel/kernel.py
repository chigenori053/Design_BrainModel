import traceback
import uuid
from typing import Any, Optional

from .types import AgentState, AgentResponse, ObservationSummary, StateMessage, SimulationError
from .state import AgentStateManager
from .observer import InputObserver
from .response import ResponseFormatter
from .lifecycle import LifecycleController
from .proposal import ProposalGenerator
from .simulation import SimulationEngine
from .design_issue import DesignIssueOrganizer
from .types import SimulationParams

class AgentKernel:
    def __init__(self):
        self.state_manager = AgentStateManager()
        self.observer = InputObserver()
        self.proposal_generator = ProposalGenerator()
        self.simulation_engine = SimulationEngine()
        self.issue_organizer = DesignIssueOrganizer()
        self.response_formatter = ResponseFormatter()
        self.lifecycle = LifecycleController(self.state_manager)
        self._active_proposal = None
        self._proposal_registry: dict[str, Any] = {}
        self._max_contexts = 3
        
        # Start the agent (INIT -> IDLE)
        self.lifecycle.startup()

    @property
    def current_state(self) -> AgentState:
        return self.state_manager.current_state

    def receive_message(self, content: Any, source: str = "user") -> AgentResponse:
        """
        Main entry point for the Agent.
        Receives input, processes it through the lifecycle, and returns a response.
        """
        try:
            with self.lifecycle.processing_cycle():
                try:
                    # --- OBSERVING Phase ---
                    # 1. Parse/Observe Input
                    observation = self.observer.observe(content, source=source)
                    
                    # --- SIMULATION & REVIEW Phase Check ---
                    # According to Phase20-D: Simulation/Traces are converted to DesignIssue
                    
                    content_str = str(content).lower()
                    is_retry = "retry" in content_str or "再シミュレーション" in content_str
                    is_simulate = "simulate" in content_str
                    if is_simulate or is_retry:
                        params = SimulationParams()
                        if "deep" in content_str:
                            params.analysis_depth = "deep"
                        proposal = self._active_proposal

                        self.state_manager.transition_to(AgentState.PROPOSING)
                        self.state_manager.transition_to(AgentState.SIMULATING)

                        if not proposal:
                            raise SimulationError("ProposalUnit required")
                        
                        sim_result = self.simulation_engine.simulate(proposal, params=params)
                        traces = self.simulation_engine.get_failure_traces(proposal.proposal_id)
                        
                        # Phase20-D: Organize issues
                        issues = self.issue_organizer.organize_issues(
                            proposal=proposal,
                            simulation_result=sim_result,
                            failure_traces=traces
                        )

                        # Clear traces once review is prepared
                        self.simulation_engine.discard_traces(proposal.proposal_id)
                        
                        # Transition to DESIGN_REVIEW via RESPONDING
                        self.state_manager.transition_to(AgentState.RESPONDING)
                        
                        return self.response_formatter.format_review_response(
                            proposal_id=proposal.proposal_id,
                            issues=issues
                        )

                    # --- PROPOSING Phase Check ---
                    # In Phase20-B, we check if input triggers a proposal
                    # If so, we transition to PROPOSING
                    
                    proposal = self.proposal_generator.generate_proposal(observation)
                    
                    if proposal:
                        if proposal.proposal_id not in self._proposal_registry:
                            if len(self._proposal_registry) >= self._max_contexts:
                                raise SimulationError("max_contexts exceeded")
                            self._proposal_registry[proposal.proposal_id] = {
                                "proposal_id": proposal.proposal_id,
                                "context_id": str(uuid.uuid4())
                            }

                        self._active_proposal = proposal

                        # Explicit transition to PROPOSING -> SIMULATING
                        self.state_manager.transition_to(AgentState.PROPOSING)
                        self.state_manager.transition_to(AgentState.SIMULATING)

                        sim_result = self.simulation_engine.simulate(proposal)
                        traces = self.simulation_engine.get_failure_traces(proposal.proposal_id)

                        issues = self.issue_organizer.organize_issues(
                            proposal=proposal,
                            simulation_result=sim_result,
                            failure_traces=traces
                        )

                        self.simulation_engine.discard_traces(proposal.proposal_id)

                        self.state_manager.transition_to(AgentState.RESPONDING)

                        return self.response_formatter.format_review_response(
                            proposal_id=proposal.proposal_id,
                            issues=issues
                        )

                    # --- Fallback: No proposal present ---
                    self.state_manager.transition_to(AgentState.PROPOSING)
                    self.state_manager.transition_to(AgentState.SIMULATING)
                    raise SimulationError("ProposalUnit required")
                except Exception:
                    if self.state_manager.current_state == AgentState.OBSERVING:
                        self.state_manager.transition_to(AgentState.PROPOSING)
                        self.state_manager.transition_to(AgentState.SIMULATING)
                        self.state_manager.transition_to(AgentState.RESPONDING)
                    elif self.state_manager.current_state == AgentState.PROPOSING:
                        self.state_manager.transition_to(AgentState.SIMULATING)
                        self.state_manager.transition_to(AgentState.RESPONDING)
                    elif self.state_manager.current_state == AgentState.SIMULATING:
                        self.state_manager.transition_to(AgentState.RESPONDING)
                    raise

        except Exception as e:
            # Error Handling: Return StateMessage with error
            try:
                if self.state_manager.current_state == AgentState.SIMULATING:
                    self.state_manager.transition_to(AgentState.RESPONDING)
            except:
                pass

            return self.response_formatter.format_state_message(
                state=self.state_manager.current_state,
                message=f"Error processing input: {str(e)}"
            )

    def shutdown(self):
        self.lifecycle.shutdown()
