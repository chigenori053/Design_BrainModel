from ..memory.types import SemanticUnit, Decision, Classification
from ..memory.gate import MemoryGate
from .base import BaseCoreB

class ValidationCore(BaseCoreB):
    """
    Core-B: Validation.
    Evaluates candidates and decides ACCEPT/REVIEW/REJECT.
    Then passes them to MemoryGate.
    """
    def __init__(self, memory_gate: MemoryGate):
        self.memory_gate = memory_gate

    def evaluate(self, unit: SemanticUnit) -> SemanticUnit:
        """
        Mock Evaluation Logic.
        Decides the fate of a unit and attempts to store it via MemoryGate.
        """
        
        # 1. Evaluate (Mock Logic)
        if "database" in unit.content.lower():
            unit.decision = Decision.ACCEPT
            unit.classification = Classification.GENERALIZABLE
        elif "constraint" in unit.type:
            unit.decision = Decision.REVIEW
            unit.classification = Classification.UNIQUE
        else:
            unit.decision = Decision.REVIEW
            unit.classification = Classification.DISCARDABLE
            
        # 2. Persist (try to store)
        # Core-B triggers the storage attempt.
        self.memory_gate.process(unit)
        
        return unit
