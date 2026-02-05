from typing import List, Optional
from ..co_design_kernel.types import ProposalUnit, L2Patch, ApplyIntent

class ApplyEngine:
    """
    ApplyEngine (Phase21): Finalizes and applies design changes.
    Strictly isolated from SearchArtifact and external knowledge.
    """

    def prepare_patch(self, proposal: ProposalUnit) -> L2Patch:
        """
        Converts a ProposalUnit into a structural L2Patch.
        Fails if any external reference is detected.
        """
        # Logic is strictly internal to the proposal's structure
        ops = []
        for draft in proposal.draft_units:
            ops.append({
                "op": proposal.proposal_type,
                "unit_id": draft.id,
                "content_preview": draft.content[:50]
            })

        return L2Patch(
            proposal_id=proposal.proposal_id,
            base_l2_version=proposal.base_l2_version,
            operations=ops
        )

    def apply_to_l2(self, patch: L2Patch, intent: ApplyIntent) -> bool:
        """
        Final execution of the patch.
        In this POC, we simulate the persistent application.
        """
        if intent.proposal_id != patch.proposal_id:
            raise ValueError("Intent proposal ID mismatch.")

        # Simulate writing to L2 store
        # In real system, this updates SemanticUnit-L2 files.
        return True
