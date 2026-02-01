import json
import time
from enum import Enum
from typing import List, Optional, Set, Dict, Any
import numpy as np
from .types import MemoryStatus, Decision, MemoryHit
from .store import CanonicalStore, QuarantineStore

class RecallPhase(str, Enum):
    PHASE_1 = "PHASE_1_CANONICAL"
    PHASE_2 = "PHASE_2_QUARANTINE"
    PHASE_3 = "PHASE_3_FROZEN"

class RecallStrategy:
    """
    Spec-05: Implements the multi-phase recall strategy based on Expected Utility (EU).
    """
    def __init__(
        self,
        theta_sim: float = 0.7,
        theta_eu: float = 0.05,
        theta_entropy: float = 0.4
    ):
        self.theta_sim = theta_sim
        self.theta_eu = theta_eu
        self.theta_entropy = theta_entropy

    def execute_recall(
        self,
        canonical: CanonicalStore,
        quarantine: QuarantineStore,
        query_vector: np.ndarray,
        current_entropy: float = 1.0,
        debug_mode: bool = False,
        human_override: bool = False
    ) -> List[Dict[str, Any]]:
        """
        Executes the three-phase recall flow.
        Returns a list of adopted memory hits with metadata.
        """
        final_candidates: List[Dict[str, Any]] = []

        # --- Phase 1: Canonical Recall ---
        canonical_hits = self._phase_1_canonical(canonical, query_vector)
        # Filter by EU_delta (In Canonical, we assume high utility if resonance is high, 
        # but Spec-05 says EU_delta > 0 is the final criteria)
        p1_adopted = self._filter_and_evaluate(canonical_hits, canonical, RecallPhase.PHASE_1)
        final_candidates.extend(p1_adopted)

        # Check success criteria for Phase 1
        if self._is_recall_sufficient(p1_adopted):
            return final_candidates

        # --- Phase 2: Quarantine Recall ---
        # Condition: P1 insufficient AND entropy >= theta_entropy
        if current_entropy >= self.theta_entropy:
            quarantine_hits = self._phase_2_quarantine(quarantine, query_vector)
            p2_adopted = self._filter_and_evaluate(quarantine_hits, quarantine, RecallPhase.PHASE_2)
            final_candidates.extend(p2_adopted)

        if not self._is_recall_sufficient(final_candidates):
            # --- Phase 3: FROZEN Recall ---
            # Condition: P1, P2 insufficient AND entropy high AND (human OR debug)
            if current_entropy > 0.8 and (human_override or debug_mode):
                frozen_hits = self._phase_3_frozen(quarantine, query_vector)
                p3_adopted = self._filter_and_evaluate(frozen_hits, quarantine, RecallPhase.PHASE_3)
                final_candidates.extend(p3_adopted)

        return final_candidates

    def _phase_1_canonical(self, store: CanonicalStore, vector: np.ndarray) -> List[MemoryHit]:
        # Canonical: Status ACTIVE, Max 5 items
        return store.recall(vector, top_k=5, include_statuses={MemoryStatus.ACTIVE})

    def _phase_2_quarantine(self, store: QuarantineStore, vector: np.ndarray) -> List[MemoryHit]:
        # Quarantine: Status ACTIVE, Max 3 items
        return store.recall(vector, top_k=3, include_statuses={MemoryStatus.ACTIVE})

    def _phase_3_frozen(self, store: QuarantineStore, vector: np.ndarray) -> List[MemoryHit]:
        # Frozen: Status FROZEN, Max 1 item
        return store.recall(vector, top_k=1, include_statuses={MemoryStatus.FROZEN})

    def _filter_and_evaluate(
        self,
        hits: List[MemoryHit],
        store: Any,
        phase: RecallPhase
    ) -> List[Dict[str, Any]]:
        adopted = []
        for hit in hits:
            unit = store.get(hit.key)
            if not unit:
                continue

            # 8.2 Recall Candidate Evaluation
            # We use avg_EU_delta from the unit as the proxy for EU_delta
            eu_delta = unit.avg_EU_delta if hasattr(unit, 'avg_EU_delta') else 0.0
            
            # Spec-05: Canonical units might not have avg_EU_delta (reset during promotion)
            # but they are inherently trusted. For Canonical, if resonance is high, we treat EU_delta as positive.
            if phase == RecallPhase.PHASE_1:
                eu_delta = max(eu_delta, 0.01) # Minimum trust for Canonical

            if eu_delta > 0:
                result = {
                    "memory_id": hit.key,
                    "store": store.store_type,
                    "status": unit.status,
                    "similarity": hit.resonance,
                    "EU_delta": eu_delta,
                    "phase": phase
                }
                # 10. Log Event: MEMORY_RECALLED
                self._log_recall(result)
                adopted.append(result)
        
        return adopted

    def _is_recall_sufficient(self, adopted: List[Dict[str, Any]]) -> bool:
        """Checks if the current recall results satisfy Phase 1 success criteria."""
        if not adopted:
            return False
        
        max_sim = max(a["similarity"] for a in adopted)
        max_eu = max(a["EU_delta"] for a in adopted)
        
        return max_sim >= self.theta_sim and max_eu >= self.theta_eu

    def _log_recall(self, result: Dict[str, Any]):
        log_entry = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "event": "MEMORY_RECALLED",
            **result
        }
        print(f"LOG: {json.dumps(log_entry)}")
