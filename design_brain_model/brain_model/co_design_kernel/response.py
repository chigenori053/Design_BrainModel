from .types import AgentState, ObservationSummary, StateMessage, ProposalUnit, ProposalResponse, SimulationResult, ReviewResponse, DesignIssue

class ResponseFormatter:
    """
    Formats internal processing results into AgentResponse objects.
    """

    @staticmethod
    def format_proposal(proposal: ProposalUnit) -> ProposalResponse:
        description = f"Generated proposal (ID: {proposal.proposal_id}) with {len(proposal.draft_units)} draft units."
        return ProposalResponse(
            proposal=proposal,
            description=description
        )

    @staticmethod
    def format_simulation_result(result: SimulationResult) -> SimulationResult:
        # For now, it just returns the result as is, but could add formatting if needed
        return result

    @staticmethod
    def format_review_response(proposal_id: str, issues: list[DesignIssue]) -> ReviewResponse:
        return ReviewResponse(
            proposal_id=proposal_id,
            issues=issues
        )

    @staticmethod
    def format_observation_summary(summary_text: str) -> ObservationSummary:
        return ObservationSummary(summary=summary_text)

    @staticmethod
    def format_state_message(state: AgentState, message: str) -> StateMessage:
        return StateMessage(state=state, message=message)
