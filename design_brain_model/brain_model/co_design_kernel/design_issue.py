from typing import List, Optional
from .types import DesignIssue, SimulationResult, FailureTrace, ProposalUnit

class DesignIssueOrganizer:
    """
    Organizes SimulationResults and FailureTraces into DesignIssues (Phase20-D).
    Strictly factual. No solutions. No priorities.
    """

    def organize_issues(self, 
                        proposal: ProposalUnit, 
                        simulation_result: Optional[SimulationResult] = None, 
                        failure_traces: Optional[List[FailureTrace]] = None) -> List[DesignIssue]:
        """
        Converts simulation data into a list of DesignIssues for human review.
        """
        issues = []
        max_issues = 5
        
        # 1. Process Simulation Issues
        if simulation_result:
            for sim_issue in simulation_result.detected_issues:
                # Limit to 5 issues as per spec
                if len(issues) >= max_issues:
                    break
                
                issues.append(DesignIssue(
                    proposal_id=proposal.proposal_id,
                    origin="simulation",
                    context_role="STRUCTURE_CHECKER", # Simplified for Phase20-D
                    issue_type=sim_issue.type,
                    description=sim_issue.description,
                    evidence={"simulation_id": simulation_result.simulation_id}
                ))

            # 2. Process Uncertainties
            for note in simulation_result.uncertainty_notes:
                if len(issues) >= max_issues:
                    break
                issues.append(DesignIssue(
                    proposal_id=proposal.proposal_id,
                    origin="simulation",
                    issue_type="uncertainty",
                    description=note,
                    evidence={"simulation_id": simulation_result.simulation_id}
                ))

        # 3. Process Failure Traces (if not already reached limit)
        if failure_traces and len(issues) < 5:
            for trace in failure_traces:
                if len(issues) >= max_issues:
                    break
                issues.append(DesignIssue(
                    proposal_id=proposal.proposal_id,
                    origin="failure_trace",
                    context_role=trace.context_role,
                    issue_type="simulation_failed",
                    description=trace.failure_reason,
                    evidence={"trace_id": trace.trace_id}
                ))

        return issues
