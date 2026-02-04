from typing import Optional, List
from .types import ProposalUnit, DraftUnit, InputData

class ProposalGenerator:
    """
    Responsible for generating safe ProposalUnits based on input observations.
    It does not modify L2 state.
    """

    def generate_proposal(self, observation: InputData) -> Optional[ProposalUnit]:
        """
        Analyzes the input and generates a ProposalUnit if criteria are met.
        For Phase20-B, we use simple keyword triggering as a placeholder for real reasoning.
        """
        content = observation.content.lower()
        trigger_keywords = ["propose", "suggest", "alternative", "draft", "plan"]
        
        if not any(keyword in content for keyword in trigger_keywords):
            return None

        # --- Sandbox Logic: Create a Mock Proposal ---
        # In a real scenario, this would read L2, reason, and create drafts.
        # Here we just demonstrate the structure.
        
        draft = DraftUnit(
            content=f"Proposed modification based on: {observation.content}",
            type="DRAFT",
            decision_polarity=True
        )
        
        return ProposalUnit(
            proposal_type="modify",
            target_units=["(simulated-target-id)"],
            draft_units=[draft],
            rationale="User explicitly requested a proposal.",
            impact_scope={
                "affected_domains": ["design"]
            },
            status="DRAFT"
        )
