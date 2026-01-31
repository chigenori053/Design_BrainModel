# design_brain_model/agent.py
"""
Phase 17-4: Agent Implementation
This module defines an auxiliary intelligence (Agent) that operates based on
a ViewModel snapshot, providing suggestions or observations without making
any decisions itself.
"""
from typing import Optional

from .brain_model.view_model import L1ContextSnapshotVM
from .command import CreateL1AtomCommand

class Agent:
    """
    An auxiliary intelligence that assists by observing the state via ViewModels.
    It does not hold state and its outputs are always treated as new L1 atoms.
    """
    def __init__(self, name: str = "AssistantAgent-01"):
        self.name = name

    def run(self, snapshot: L1ContextSnapshotVM) -> Optional[CreateL1AtomCommand]:
        """
        Analyzes a snapshot of the context and produces a command to create a
        new L1 atom if it has a suggestion.

        Args:
            snapshot: The read-only ViewModel of the current context.

        Returns:
            A CreateL1AtomCommand if the agent has a suggestion, otherwise None.
        """
        # --- Agent Logic Example: Pointing out missing information ---
        if snapshot.missing_types:
            missing_type_str = ", ".join(snapshot.missing_types)
            content = f"Observation: The current context appears to be missing information of the following types: {missing_type_str}."
            
            # As per the spec, Agent output is treated as an L1 Atom.
            # We generate a command to create this atom.
            return CreateL1AtomCommand(
                content=content,
                type="OBSERVATION",
                source=self.name
            )

        # --- Another example: Suggesting a next step based on entropy ---
        if snapshot.entropy_summary > 0.7:
             content = f"Question: The context entropy is high ({snapshot.entropy_summary:.2f}). Should we focus on clustering existing atoms to reduce uncertainty?"
             return CreateL1AtomCommand(
                content=content,
                type="QUESTION",
                source=self.name
             )

        return None
