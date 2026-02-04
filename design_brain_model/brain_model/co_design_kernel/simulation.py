from typing import List, Optional, Dict
from .types import ProposalUnit, SimulationResult, SimulationIssue, SimulationContext, DraftUnit, FailureTrace, SimulationParams

class SimulationEngine:
    """
    Implements Design Simulation (Phase20-C) with Failure Trace Addendum.
    Allows predicting impact without modification and tracking ephemeral failure traces.
    """

    def __init__(self):
        # Ephemeral Trace Store: persistence: none, learning_input: false
        self._trace_store: Dict[str, List[FailureTrace]] = {}

    def simulate(self, proposal: ProposalUnit, params: Optional[SimulationParams] = None) -> SimulationResult:
        """
        Runs a simulation based on the provided proposal and optional parameters.
        """
        if not proposal:
            raise ValueError("Proposal is required for simulation.")
        
        current_params = params or SimulationParams()

        # 1. Create Simulation Context (Shadow World)
        context = SimulationContext(
            proposal_id=proposal.proposal_id,
            base_l2_version=proposal.base_l2_version,
            shadow_l2_drafts=proposal.draft_units,
            status="RUNNING",
            context_role="STRUCTURE_CHECKER",
            params=current_params
        )

        # 2. Simulation Logic (Phase20-C rules)
        try:
            # Trigger failure if content explicitly mentions "fail simulation" for testing
            if any("fail simulation" in d.content.lower() for d in proposal.draft_units):
                raise RuntimeError("Deliberate simulation failure for testing.")

            issues = self._detect_design_issues(context)
            uncertainty = self._collect_uncertainties(context)
            summary = self._generate_simulation_summary(proposal, issues)
            context.status = "COMPLETED"

            return SimulationResult(
                simulation_id=context.simulation_id,
                summary=summary,
                detected_issues=issues,
                uncertainty_notes=uncertainty
            )

        except Exception as e:
            context.status = "FAILED"
            trace = self._record_failure_trace(context, str(e))
            return SimulationResult(
                simulation_id=context.simulation_id,
                summary=f"Simulation FAILED: {str(e)}",
                detected_issues=[SimulationIssue(type="simulation_failed", description=str(e))],
                uncertainty_notes=["Reason for failure might be structural complexity."],
                type="SIMULATION_RESULT" # Ensure type is set
            )

    def _record_failure_trace(self, context: SimulationContext, reason: str) -> FailureTrace:
        """
        Generates and stores a Failure Trace.
        """
        trace = FailureTrace(
            proposal_id=context.proposal_id,
            context_id=context.simulation_id,
            context_role=context.context_role,
            failure_reason=reason,
            parameters_snapshot={
                "simulation_scope": context.params.simulation_scope,
                "analysis_depth": context.params.analysis_depth,
                "issue_filter": context.params.issue_filter
            }
        )
        if trace.proposal_id not in self._trace_store:
            self._trace_store[trace.proposal_id] = []
        self._trace_store[trace.proposal_id].append(trace)
        return trace

    def get_failure_traces(self, proposal_id: str) -> List[FailureTrace]:
        """Returns the ephemeral failure traces scoped to a proposal."""
        return list(self._trace_store.get(proposal_id, []))

    def discard_traces(self, proposal_id: str) -> None:
        """Manually discards failure traces for a proposal."""
        if proposal_id in self._trace_store:
            self._trace_store[proposal_id] = []

    def _detect_design_issues(self, context: SimulationContext) -> List[SimulationIssue]:
        """
        Detects structural issues in the 'Shadow L2'.
        """
        issues = []
        
        # Heuristic: If multiple draft units, check for potential ambiguity
        if len(context.shadow_l2_drafts) > 1:
            issues.append(SimulationIssue(
                type="ambiguity",
                description="Draft unit count > 1; responsibility boundaries are not represented in shadow L2."
            ))
        
        # Mock dependency check: if content contains 'dependency', simulate a break
        for draft in context.shadow_l2_drafts:
            if "dependency" in draft.content.lower():
                issues.append(SimulationIssue(
                    type="dependency_break",
                    description=f"Draft content contains 'dependency' token in unit {draft.id}; dependency graph not validated."
                ))

        return issues

    def _collect_uncertainties(self, context: SimulationContext) -> List[str]:
        """
        Lists things the simulation cannot predict.
        """
        return [
            "Runtime side-effects of code execution",
            "Performance impact on large datasets",
            "Human interpretation of the changed design"
        ]

    def _generate_simulation_summary(self, proposal: ProposalUnit, issues: List[SimulationIssue]) -> str:
        issue_count = len(issues)
        if issue_count == 0:
            return f"Simulation of proposal {proposal.proposal_id} suggests a stable structural transition."
        else:
            return f"Simulation of proposal {proposal.proposal_id} identified {issue_count} potential structural risks."
