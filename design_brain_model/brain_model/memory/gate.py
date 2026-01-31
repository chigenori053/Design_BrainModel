from .types import SemanticUnit, Decision, Classification, MemoryType
from .space import MemorySpace

class MemoryGate:
    """
    Controls what enters the MemorySpace.
    Enforces the 'Passive Memory' rule.
    """
    def __init__(self, memory_space: MemorySpace):
        self.memory_space = memory_space

    def process(self, unit: SemanticUnit) -> bool:
        """
        Main entry point for storing a SemanticUnit into memory.
        Routes based on Decision and Classification.
        """
        if not unit.decision:
            # Undecided units cannot enter memory
            return False

        # 1. Decision Filter
        if unit.decision == Decision.REJECT:
            if unit.classification == Classification.DISCARDABLE:
                return False # Discard
            elif unit.classification == Classification.UNIQUE:
                # Store strict negative constraints or dangerous patterns? 
                # For Phase 9 MVP, we might store rejected unique items in PHS for history.
                self.memory_space.quarantine.add(unit)
                return True
            # Generalizable rejected items? Likely rare, but store in PHS.
            self.memory_space.quarantine.add(unit)
            return True

        if unit.decision == Decision.REVIEW:
            # Review items go to PHS if they have potential value
            if unit.classification != Classification.DISCARDABLE:
                 self.memory_space.quarantine.add(unit)
                 return True
            return False

        if unit.decision == Decision.ACCEPT:
            # Accepted items ALWAYS go to PHS first.
            self.memory_space.canonical.add(unit)
            
            # If Generalizable, they are CANDIDATES for SHM, but not immediate.
            # Promotion is a separate step in Phase 9.
            return True

        return False
