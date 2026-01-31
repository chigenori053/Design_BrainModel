from abc import ABC, abstractmethod
from typing import Dict, List, Optional, Any, Protocol
from pathlib import Path
import numpy as np
import json
import os

from .types import SemanticUnit, StoreType, MemoryHit
from .persistent_store import FileHolographicStore, HolographicTrace

class MemoryStore(Protocol):
    """
    Common interface for all memory stores (Spec-01).
    """
    def add(self, unit: SemanticUnit, vector: Optional[np.ndarray] = None) -> None:
        ...

    def recall(self, query_vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
        ...

    def load(self) -> None:
        ...

    def flush(self) -> None:
        ...

class BaseStore(ABC):
    def __init__(self, store_type: StoreType):
        self.store_type = store_type

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
                timestamp=0 # Should be current time
            )
            self.vector_store.append(trace)

    def recall(self, query_vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
        results = self.vector_store.recall(query_vector, k=top_k)
        return [MemoryHit(key=r.source_unit_id, resonance=r.resonance) for r in results]

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
                timestamp=0
            )
            self.vector_store.append(trace)

    def recall(self, query_vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
        results = self.vector_store.recall(query_vector, k=top_k)
        return [MemoryHit(key=r.source_unit_id, resonance=r.resonance) for r in results]

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

    def recall(self, query_vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
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

    def load(self) -> None:
        # Re-initializes/clears on load as it's transient
        self.items = []

    def flush(self) -> None:
        # No-op for working memory
        pass