from ..memory.types import SemanticUnit, Decision, Classification, DecisionResult
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
        label = Decision.REVIEW
        classification = Classification.DISCARDABLE
        
        if "database" in unit.content.lower():
            label = Decision.ACCEPT
            classification = Classification.GENERALIZABLE
        elif "constraint" in unit.type:
            label = Decision.REVIEW
            classification = Classification.UNIQUE
        
        unit.classification = classification
        unit.decision_label = label # Pre-set for local context if needed

        # Create DecisionResult (Mock values for confidence/entropy)
        decision_result = DecisionResult(
            label=label,
            confidence=0.8 if label == Decision.ACCEPT else 0.5,
            entropy=0.2 if label == Decision.ACCEPT else 0.8,
            utility=0.5,
            reason="Mock evaluation logic in BCycle"
        )
            
        # 2. Persist (try to store)
        # Core-B triggers the storage attempt.
        self.memory_gate.process(unit, decision_result)
        
        return unit
