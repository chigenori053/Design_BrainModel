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
from .types import SimulationParams, ApplyIntent, L2Patch
from ..phase21.apply_engine import ApplyEngine
from ..phase22.agent import SearchAgent
from ..phase22.types import SearchRequest

class AgentKernel:
    def __init__(self):
        self.state_manager = AgentStateManager()
        self.observer = InputObserver()
        self.proposal_generator = ProposalGenerator()
        self.simulation_engine = SimulationEngine()
        self.issue_organizer = DesignIssueOrganizer()
        self.apply_engine = ApplyEngine()
        self.search_agent = SearchAgent()
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
        # Capture pre-lifecycle state for trigger evaluation
        pre_state = self.current_state
        apply_states = {
            AgentState.APPLY_PREPARE,
            AgentState.APPLY_CONFIRM,
            AgentState.APPLY_EXECUTE,
            AgentState.APPLY_DONE
        }
        
        try:
            with self.lifecycle.processing_cycle():
                content_str = str(content).lower()

                # --- APPLY Phase Blocking & Handling (Phase21) ---
                if pre_state in apply_states:
                    if "search" in content_str:
                        return self.response_formatter.format_state_message(
                            state=pre_state,
                            message="Search operations are strictly prohibited during the application phase."
                        )

                    if pre_state == AgentState.APPLY_CONFIRM and "confirm" in content_str:
                        self.state_manager.transition_to(AgentState.APPLY_EXECUTE)
                        intent = ApplyIntent(proposal_id=self._active_proposal.proposal_id)
                        patch = self.apply_engine.prepare_patch(self._active_proposal)
                        self.apply_engine.apply_to_l2(patch, intent)
                        self.state_manager.transition_to(AgentState.APPLY_DONE)
                        self.state_manager.transition_to(AgentState.DESIGN_REVIEW)
                        return self.response_formatter.format_state_message(
                            state=self.state_manager.current_state,
                            message="Design application COMPLETED. L2 has been updated."
                        )

                    return self.response_formatter.format_state_message(
                        state=pre_state,
                        message="Application in progress. Awaiting 'confirm' to execute."
                    )
                
                # --- 1. SEARCH Agent Handling (Phase22) ---
                if "search" in content_str:
                    request = SearchRequest(query=content_str, requested_by="human")
                    artifact = self.search_agent.execute_search(request)
                    # For search, we need to end the cycle properly
                    self.state_manager.transition_to(AgentState.RESPONDING)
                    return self.response_formatter.format_observation_summary(
                        f"Observation complete. Artifact {artifact.artifact_id} generated. Human review required."
                    )

                # --- 2. APPLY Flow Handling (Phase21) ---
                if "apply" in content_str and pre_state != AgentState.DESIGN_REVIEW:
                    self.state_manager.transition_to(AgentState.RESPONDING)
                    return self.response_formatter.format_state_message(
                        state=self.state_manager.current_state,
                        message="Apply is only allowed from DESIGN_REVIEW with an active proposal."
                    )

                if "apply" in content_str and pre_state == AgentState.DESIGN_REVIEW:
                    if not self._active_proposal:
                        raise SimulationError("No active proposal to apply.")
                    
                    self.state_manager.transition_to(AgentState.APPLY_PREPARE)
                    patch = self.apply_engine.prepare_patch(self._active_proposal)
                    self.state_manager.transition_to(AgentState.APPLY_CONFIRM)
                    
                    # STAY in APPLY_CONFIRM (lifecycle allows it)
                    return self.response_formatter.format_state_message(
                        state=self.state_manager.current_state,
                        message=f"L2Patch {patch.patch_id} prepared. Type 'confirm' to execute."
                    )

                # --- 3. OBSERVING Phase & Standard Proposal Logic ---
                observation = self.observer.observe(content, source=source)
                
                # SIMULATION / RETRY Logic
                is_retry = "retry" in content_str or "再シミュレーション" in content_str
                is_simulate = "simulate" in content_str
                if is_simulate or is_retry:
                    if not self._active_proposal:
                        raise SimulationError("ProposalUnit required")
                    self.state_manager.transition_to(AgentState.PROPOSING)
                    self.state_manager.transition_to(AgentState.SIMULATING)
                    
                    params = SimulationParams()
                    if "deep" in content_str:
                        params.analysis_depth = "deep"
                    
                    sim_result = self.simulation_engine.simulate(self._active_proposal, params=params)
                    traces = self.simulation_engine.get_failure_traces(self._active_proposal.proposal_id)
                    issues = self.issue_organizer.organize_issues(self._active_proposal, sim_result, traces)
                    self.simulation_engine.discard_traces(self._active_proposal.proposal_id)
                    
                    self.state_manager.transition_to(AgentState.RESPONDING)
                    return self.response_formatter.format_review_response(self._active_proposal.proposal_id, issues)

                # Standard Proposal Generation
                proposal = self.proposal_generator.generate_proposal(observation)
                if proposal:
                    self._active_proposal = proposal
                    self.state_manager.transition_to(AgentState.PROPOSING)
                    self.state_manager.transition_to(AgentState.SIMULATING)
                    sim_result = self.simulation_engine.simulate(proposal)
                    traces = self.simulation_engine.get_failure_traces(proposal.proposal_id)
                    issues = self.issue_organizer.organize_issues(proposal, sim_result, traces)
                    self.simulation_engine.discard_traces(proposal.proposal_id)
                    self.state_manager.transition_to(AgentState.RESPONDING)
                    return self.response_formatter.format_review_response(proposal.proposal_id, issues)

                # Default fallback
                self.state_manager.transition_to(AgentState.RESPONDING)
                return self.response_formatter.format_observation_summary(f"Acknowledged: {content[:20]}...")

        except Exception as e:
            # Emergency cleanup
            try:
                if self.state_manager.current_state == AgentState.OBSERVING:
                    self.state_manager.transition_to(AgentState.RESPONDING)
                if self.state_manager.current_state == AgentState.RESPONDING:
                    self.state_manager.transition_to(AgentState.DESIGN_REVIEW)
            except:
                pass
            return self.response_formatter.format_state_message(
                state=self.state_manager.current_state,
                message=f"Error processing input: {str(e)}"
            )

    def shutdown(self):
        self.lifecycle.shutdown()
