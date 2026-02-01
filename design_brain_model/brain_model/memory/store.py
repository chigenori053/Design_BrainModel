from abc import ABC, abstractmethod
from typing import Dict, List, Optional, Any, Protocol, Set
from pathlib import Path
import numpy as np
import json
import os
import time

from .types import SemanticUnit, StoreType, MemoryHit, MemoryStatus
from .persistent_store import FileHolographicStore, HolographicTrace

class MemoryStore(Protocol):
    """
    Common interface for all memory stores (Spec-01).
    """
    def add(self, unit: SemanticUnit, vector: Optional[np.ndarray] = None) -> None:
        ...

    def get(self, unit_id: str) -> Optional[SemanticUnit]:
        ...

    def recall(
        self,
        query_vector: np.ndarray,
        top_k: int = 1,
        include_statuses: Optional[Set[MemoryStatus]] = None
    ) -> List[MemoryHit]:
        ...

    def load(self) -> None:
        ...

    def flush(self) -> None:
        ...

    def update_status(self, unit_id: str, new_status: MemoryStatus, reason: str, human_override: bool = False) -> bool:
        ...

class BaseStore(ABC):
    def __init__(self, store_type: StoreType):
        self.store_type = store_type

    def _validate_transition(self, current: MemoryStatus, to: MemoryStatus, human_override: bool) -> bool:
        """Enforces Spec-02 transition rules."""
        if current == to:
            return True
        
        # ACTIVE -> FROZEN / DISABLED (Always allowed)
        if current == MemoryStatus.ACTIVE and to in [MemoryStatus.FROZEN, MemoryStatus.DISABLED]:
            return True
        
        # FROZEN -> ACTIVE (Human only)
        if current == MemoryStatus.FROZEN and to == MemoryStatus.ACTIVE:
            return human_override
        
        # FROZEN -> DISABLED (Allowed)
        if current == MemoryStatus.FROZEN and to == MemoryStatus.DISABLED:
            return True
        
        # DISABLED -> ANY (Human only)
        if current == MemoryStatus.DISABLED:
            return human_override
            
        return False

class PersistentUnitStore:
    """Helper to persist SemanticUnits to a JSON line file."""
    def __init__(self, file_path: Path):
        self.file_path = file_path
        self.cache: Dict[str, SemanticUnit] = {}
        self.file_path.parent.mkdir(parents=True, exist_ok=True)
        self.load()

    def load(self):
        if not self.file_path.exists():
            return
        with open(self.file_path, 'r', encoding='utf-8') as f:
            for line in f:
                if line.strip():
                    try:
                        data = json.loads(line)
                        unit = SemanticUnit(**data)
                        self.cache[unit.id] = unit
                    except Exception:
                        continue # Skip bad lines

    def save(self, unit: SemanticUnit):
        self.cache[unit.id] = unit
        with open(self.file_path, 'a', encoding='utf-8') as f:
            f.write(unit.model_dump_json() + "\n")

    def get(self, unit_id: str) -> Optional[SemanticUnit]:
        return self.cache.get(unit_id)

class CanonicalStore(BaseStore):
    """
    CanonicalStore: Accepted memories. Append-only, persistent, high reliability.
    """
    def __init__(self, persistence_dir: Path):
        super().__init__(StoreType.CANONICAL)
        self.vector_store = FileHolographicStore(store_dir=persistence_dir / "canonical_vectors")
        self.unit_store = PersistentUnitStore(persistence_dir / "canonical_units.jsonl")

    def add(self, unit: SemanticUnit, vector: Optional[np.ndarray] = None) -> None:
        # CanonicalStore is always ACTIVE as per Spec-02
        unit.status = MemoryStatus.ACTIVE
        
        # 1. Persist the unit content
        self.unit_store.save(unit)

        # 2. Persist vector if available
        if vector is not None:
            trace = HolographicTrace(
                trace_id=None, # Auto-generate
                source_unit_id=unit.id,
                raw_vector=vector,
                interference_vector=None,
                energy=1.0, # High confidence for Canonical
                timestamp=int(time.time()), # D-1: Real timestamp
                status=MemoryStatus.ACTIVE
            )
            self.vector_store.append(trace)

    def get(self, unit_id: str) -> Optional[SemanticUnit]:
        return self.unit_store.get(unit_id)

    def recall(
        self,
        query_vector: np.ndarray,
        top_k: int = 1,
        include_statuses: Optional[Set[MemoryStatus]] = None
    ) -> List[MemoryHit]:
        # CanonicalStore only allows ACTIVE
        results = self.vector_store.recall(query_vector, k=top_k, include_statuses={MemoryStatus.ACTIVE})
        return [MemoryHit(key=r.source_unit_id, resonance=r.resonance) for r in results]

    def update_status(self, unit_id: str, new_status: MemoryStatus, reason: str, human_override: bool = False) -> bool:
        """
        CanonicalStore items must be ACTIVE. 
        Changing status means they should likely move to QuarantineStore (handled by higher layer).
        Within CanonicalStore, we only allow ACTIVE.
        """
        if new_status != MemoryStatus.ACTIVE:
            # Spec-02: CanonicalStore is ACTIVE only.
            return False
        return True

    def load(self) -> None:
        self.vector_store.load()
        self.unit_store.load()

    def flush(self) -> None:
        self.vector_store.flush()
        # Unit store is append-only line based, so flush implies OS flush if needed, but 'a' mode handles it mostly.

class QuarantineStore(BaseStore):
    """
    QuarantineStore: Review/Reject memories. Persistent, allows metadata updates.
    """
    def __init__(self, persistence_dir: Path):
        super().__init__(StoreType.QUARANTINE)
        self.vector_store = FileHolographicStore(store_dir=persistence_dir / "quarantine_vectors")
        self.unit_store = PersistentUnitStore(persistence_dir / "quarantine_units.jsonl")

    def add(self, unit: SemanticUnit, vector: Optional[np.ndarray] = None) -> None:
        # 1. Persist the unit content
        self.unit_store.save(unit)

        # 2. Persist vector if available
        if vector is not None:
            trace = HolographicTrace(
                trace_id=None,
                source_unit_id=unit.id,
                raw_vector=vector,
                interference_vector=None,
                energy=0.5, # Lower confidence
                timestamp=int(time.time()), # D-1: Real timestamp
                status=unit.status
            )
            self.vector_store.append(trace)

    def get(self, unit_id: str) -> Optional[SemanticUnit]:
        return self.unit_store.get(unit_id)

    def record_usage(self, unit_id: str, accepted: bool, eu_delta: float) -> bool:
        """Spec-04: Update evaluation metrics in QuarantineStore."""
        unit = self.unit_store.get(unit_id)
        if not unit or unit.status != MemoryStatus.ACTIVE:
            return False
        
        unit.reuse_count += 1
        if accepted:
            unit.accept_support_count += 1
        else:
            unit.reject_impact_count += 1
        
        # Running average for avg_EU_delta
        # (new_avg = old_avg + (new_val - old_avg) / count)
        count = unit.reuse_count
        unit.avg_EU_delta = unit.avg_EU_delta + (eu_delta - unit.avg_EU_delta) / count
        
        self.unit_store.save(unit)
        return True

    def promote_to_canonical(self, unit_id: str, destination: 'CanonicalStore') -> bool:
        """Spec-04: Promote valid Quarantine memory to CanonicalStore."""
        unit = self.unit_store.get(unit_id)
        if not unit or unit.status != MemoryStatus.ACTIVE:
            return False
        
        # 6. Promotion Criteria
        if not (
            unit.confidence_init >= 0.40 and
            unit.reuse_count >= 2 and
            unit.accept_support_count >= 1 and
            unit.avg_EU_delta >= 0.05 and
            unit.reject_impact_count == 0
        ):
            return False
        
        # B-1: Safe vector retrieval via API
        trace = self.vector_store.get_trace_by_source_unit_id(unit_id)
        if not trace:
            return False
        vector = trace.raw_vector
        
        # 7. Promotion Operation
        # Create a copy for Canonical
        promoted_unit = unit.model_copy(deep=True)
        promoted_unit.status = MemoryStatus.ACTIVE
        promoted_unit.status_reason = "promoted_from_quarantine"
        promoted_unit.status_changed_at = time.time()
        
        # Reset metrics for Canonical (Spec-04: metrics not carried over)
        promoted_unit.reuse_count = 0
        promoted_unit.accept_support_count = 0
        promoted_unit.reject_impact_count = 0
        promoted_unit.avg_EU_delta = 0.0
        promoted_unit.retention_score = 0.0
        
        destination.add(promoted_unit, vector=vector)
        
        # 8. Log Event: MEMORY_PROMOTED
        log_entry = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "event": "MEMORY_PROMOTED",
            "memory_id": unit_id,
            "from": "QUARANTINE",
            "to": "CANONICAL",
            "metrics": {
                "confidence_init": unit.confidence_init,
                "reuse_count": unit.reuse_count,
                "accept_support_count": unit.accept_support_count,
                "avg_EU_delta": unit.avg_EU_delta,
                "reject_impact_count": unit.reject_impact_count
            },
            "reason": "promotion_criteria_satisfied"
        }
        print(f"LOG: {json.dumps(log_entry)}")
        
        return True

    def recall(
        self,
        query_vector: np.ndarray,
        top_k: int = 1,
        include_statuses: Optional[Set[MemoryStatus]] = None
    ) -> List[MemoryHit]:
        # QuarantineStore allows any status, but defaults to ACTIVE if not specified
        results = self.vector_store.recall(query_vector, k=top_k, include_statuses=include_statuses)
        return [MemoryHit(key=r.source_unit_id, resonance=r.resonance) for r in results]

    def update_status(self, unit_id: str, new_status: MemoryStatus, reason: str, human_override: bool = False) -> bool:
        unit = self.unit_store.get(unit_id)
        if not unit:
            return False
        
        if not self._validate_transition(unit.status, new_status, human_override):
            return False
        
        # Update unit
        old_status = unit.status
        unit.status = new_status
        unit.status_reason = reason
        unit.status_changed_at = time.time() # C-1: Update timestamp
        self.unit_store.save(unit)

        # Update vector store (find corresponding trace)
        # In this implementation, source_unit_id is used to find the trace.
        # Note: A unit might have multiple traces if updated, but here we assume 1:1 for simplicity.
        for trace in self.vector_store._traces:
            if trace.source_unit_id == unit_id:
                self.vector_store.update_status(trace.trace_id, new_status)
        
        # Log event (Placeholder for Spec-02 Requirement 8)
        print(f"LOG: MEMORY_STATUS_CHANGED unit={unit_id} from={old_status} to={new_status} reason={reason}")
        
        return True

    def load(self) -> None:
        self.vector_store.load()
        self.unit_store.load()

    def flush(self) -> None:
        self.vector_store.flush()

class WorkingMemory(BaseStore):
    """
    WorkingMemory: Transient, inference only. No persistence.
    """
    def __init__(self):
        super().__init__(StoreType.WORKING)
        self.items: List[Dict[str, Any]] = []

    def add(self, unit: SemanticUnit, vector: Optional[np.ndarray] = None) -> None:
        # Working memory might store the object itself + vector
        self.items.append({"unit": unit, "vector": vector})

    def recall(
        self,
        query_vector: np.ndarray,
        top_k: int = 1,
        include_statuses: Optional[Set[MemoryStatus]] = None
    ) -> List[MemoryHit]:
        # Simple linear scan for working memory
        hits = []
        if query_vector is None:
             return []
        
        norm_query = np.linalg.norm(query_vector)
        if norm_query == 0:
            return []
        query_hat = query_vector / norm_query

        for item in self.items:
            vec = item["vector"]
            if vec is not None:
                norm_vec = np.linalg.norm(vec)
                if norm_vec > 0:
                    sim = np.dot(query_hat, vec / norm_vec)
                    hits.append(MemoryHit(key=item["unit"].id, resonance=float(sim)))
        
        hits.sort(key=lambda x: x.resonance, reverse=True)
        return hits[:top_k]

    def update_status(self, unit_id: str, new_status: MemoryStatus, reason: str, human_override: bool = False) -> bool:
        # Working memory is transient and doesn't manage lifecycle status in the same way.
        for item in self.items:
            if item["unit"].id == unit_id:
                item["unit"].status = new_status
                return True
        return False

    def load(self) -> None:
        # Re-initializes/clears on load as it's transient
        self.items = []

    def flush(self) -> None:
        # No-op for working memory
        pass