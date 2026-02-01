import json
import time
from typing import Optional
import numpy as np
from .types import SemanticUnit, Decision, DecisionResult, MemoryStatus
from .space import MemorySpace

class MemoryGate:
    """
    Controls what enters the MemorySpace.
    Enforces the 'Passive Memory' rule.
    (Spec-03: Decision -> Memory Routing)
    """
    def __init__(self, memory_space: MemorySpace):
        self.memory_space = memory_space

    def process(self, unit: SemanticUnit, decision: DecisionResult, vector: Optional[np.ndarray] = None) -> bool:
        """
        Main entry point for storing a SemanticUnit into memory.
        Routes based on Decision and initializes metadata.
        """
        # Spec-03: Initialize metadata
        unit.decision_label = decision.label
        unit.confidence_init = decision.confidence
        unit.decision_reason = decision.reason
        
        # Spec-02/03: Initialize status to ACTIVE
        unit.status = MemoryStatus.ACTIVE
        unit.status_reason = f"initial_decision:{decision.label.value}"
        unit.status_changed_at = time.time()

        # 5. Decision -> Store Routing Rules
        store_name = ""
        if decision.label == Decision.ACCEPT:
            store = self.memory_space.canonical
            store_name = "CANONICAL"
        elif decision.label in [Decision.REVIEW, Decision.REJECT]:
            store = self.memory_space.quarantine
            store_name = "QUARANTINE"
        else:
            # 9.1 Invalid DecisionLabel
            print(f"ERROR: Invalid DecisionLabel detected: {decision.label}")
            return False

        # 5.2 Invariant: CanonicalStore is always ACTIVE
        # store.add will handle the physical persistence
        try:
            store.add(unit, vector=vector)
        except Exception as e:
            # 9.2 Store Saving Failure
            print(f"ERROR: Memory storage failed for unit {unit.id}: {e}")
            return False

        # 8. Log Event: MEMORY_ROUTED
        log_entry = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "event": "MEMORY_ROUTED",
            "memory_id": unit.id,
            "decision": decision.label,
            "store": store_name,
            "status": unit.status,
            "confidence": decision.confidence,
            "entropy": decision.entropy,
            "utility": decision.utility,
            "reason": decision.reason
        }
        print(f"LOG: {json.dumps(log_entry)}")
        
        return True
