from __future__ import annotations

from dataclasses import dataclass

"""
LEGACY MODULE (FROZEN)

This module belongs to the pre–Spec-01 memory implementation.
It is frozen and MUST NOT be used together with the Spec-01~05
HolographicMemory system.
"""

from pathlib import Path
from typing import Dict, List, Optional, Any, Union

import json
import os
import struct
import time
import uuid

import numpy as np

from .types import Decision


SCHEMA_VERSION = 1


class StoreType(str):
    CANONICAL = "CANONICAL"
    QUARANTINE = "QUARANTINE"


class MemoryStatus(str):
    ACTIVE = "ACTIVE"
    FROZEN = "FROZEN"
    DISABLED = "DISABLED"


@dataclass
class MemoryRecord:
    memory_id: str
    vector: np.ndarray
    store_type: StoreType
    status: MemoryStatus
    decision: Decision
    confidence_init: float
    retention_score: float
    counters: Dict[str, float]
    timestamp: int
    version: int = SCHEMA_VERSION

    def __post_init__(self) -> None:
        if not self.memory_id:
            raise ValueError("memory_id is required")
        self.vector = np.asarray(self.vector, dtype=np.float32)
        if self.vector.ndim != 1 or self.vector.size == 0:
            raise ValueError("vector must be a non-empty 1D float32 array")
        self.confidence_init = float(self.confidence_init)
        self.retention_score = float(self.retention_score)
        self.timestamp = int(self.timestamp)
        self.version = int(self.version)
        if not isinstance(self.counters, dict):
            raise ValueError("counters must be a dict")


@dataclass
class RecallHit:
    memory_id: str
    store_type: StoreType
    status: MemoryStatus
    resonance: float
    decision: Decision
    confidence_init: float
    retention_score: float
    counters: Dict[str, float]
    timestamp: int


@dataclass
class RecallContext:
    canonical_min_similarity: float
    entropy: float
    expected_utility_improve: bool
    allow_frozen: bool


class EventLogger:
    """状態遷移のログを append-only で記録する。"""

    def __init__(self, log_path: Path | str) -> None:
        self.log_path = Path(log_path)
        self.log_path.parent.mkdir(parents=True, exist_ok=True)

    def write(self, event_type: str, payload: Dict[str, object]) -> None:
        record = {
            "event_type": event_type,
            "timestamp": int(time.time()),
            **payload,
        }
        with self.log_path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")


class FileMemoryStore:
    """append-only な永続化ストア。"""

    MEMORY_FILENAME = "memories.bin"
    INDEX_FILENAME = "index.bin"
    META_FILENAME = "meta.json"

    def __init__(self, store_dir: Path | str, store_type: StoreType) -> None:
        self.store_dir = Path(store_dir)
        self.store_dir.mkdir(parents=True, exist_ok=True)
        self.store_type = store_type
        self.memory_path = self.store_dir / self.MEMORY_FILENAME
        self.index_path = self.store_dir / self.INDEX_FILENAME
        self.meta_path = self.store_dir / self.META_FILENAME

        self._records: List[MemoryRecord] = []
        self._offsets: List[int] = []
        self._vector_dim: Optional[int] = None
        self._loaded = False

    def load(self) -> None:
        if self.meta_path.exists():
            meta = json.loads(self.meta_path.read_text(encoding="utf-8"))
            version = int(meta.get("schema_version", 0))
            if version != SCHEMA_VERSION:
                raise RuntimeError(
                    f"Schema version mismatch: expected {SCHEMA_VERSION}, found {version}"
                )

        if not self.memory_path.exists():
            self._loaded = True
            return

        self._records = []
        self._offsets = []
        self._vector_dim = None

        with self.memory_path.open("rb") as f:
            file_size = os.fstat(f.fileno()).st_size
            while f.tell() < file_size:
                offset = f.tell()
                record = self._read_record(f)
                self._offsets.append(offset)
                self._records.append(record)
                if self._vector_dim is None:
                    self._vector_dim = int(record.vector.size)

        self._loaded = True

    def append(self, record: MemoryRecord) -> None:
        if not self._loaded:
            self.load()
        if record.version != SCHEMA_VERSION:
            raise ValueError("Record version mismatch with store schema")
        if record.store_type != self.store_type:
            raise ValueError("record.store_type does not match store")
        if self._vector_dim is None:
            self._vector_dim = int(record.vector.size)
        elif self._vector_dim != int(record.vector.size):
            raise ValueError("vector dimension does not match existing store")

        with self.memory_path.open("ab") as f:
            offset = f.tell()
            encoded = self._encode_record(record)
            f.write(encoded)
            f.flush()

        with self.index_path.open("ab") as index_f:
            index_f.write(self._encode_index_record(offset, record))
            index_f.flush()

        self._offsets.append(offset)
        self._records.append(record)

    def recall(
        self,
        query_vector: Sequence[float],
        k: int,
        include_status: Iterable[MemoryStatus],
    ) -> List[RecallHit]:
        if not self._loaded:
            self.load()
        if k <= 0:
            return []

        query = np.asarray(query_vector, dtype=np.float32)
        if query.ndim != 1 or query.size == 0:
            raise ValueError("query_vector must be a non-empty 1D float32 array")
        if self._vector_dim is not None and query.size != self._vector_dim:
            raise ValueError("query_vector dimension does not match store")

        query_norm = np.linalg.norm(query)
        if query_norm == 0:
            return []
        query = query / query_norm

        allowed = set(include_status)
        scored: List[Tuple[int, float]] = []
        for idx, record in enumerate(self._records):
            if record.status not in allowed:
                continue
            vec_norm = np.linalg.norm(record.vector)
            if vec_norm == 0:
                continue
            resonance = float(np.dot(query, record.vector / vec_norm))
            scored.append((idx, resonance))

        scored.sort(key=lambda item: item[1], reverse=True)
        results: List[RecallHit] = []
        for idx, resonance in scored[:k]:
            record = self._records[idx]
            results.append(
                RecallHit(
                    memory_id=record.memory_id,
                    store_type=record.store_type,
                    status=record.status,
                    resonance=resonance,
                    decision=record.decision,
                    confidence_init=record.confidence_init,
                    retention_score=record.retention_score,
                    counters=dict(record.counters),
                    timestamp=record.timestamp,
                )
            )
        return results

    def latest_state(self) -> Dict[str, MemoryRecord]:
        if not self._loaded:
            self.load()
        latest: Dict[str, MemoryRecord] = {}
        for record in self._records:
            prev = latest.get(record.memory_id)
            if prev is None or record.timestamp >= prev.timestamp:
                latest[record.memory_id] = record
        return latest

    def flush(self) -> None:
        if not self._loaded:
            return
        meta = {
            "schema_version": SCHEMA_VERSION,
            "record_count": len(self._records),
            "vector_dim": self._vector_dim,
            "store_type": self.store_type,
            "last_timestamp": self._records[-1].timestamp if self._records else None,
        }
        self.meta_path.write_text(json.dumps(meta, indent=2), encoding="utf-8")

    def _encode_record(self, record: MemoryRecord) -> bytes:
        meta = {
            "memory_id": record.memory_id,
            "store_type": record.store_type,
            "status": record.status,
            "decision": record.decision,
            "confidence_init": record.confidence_init,
            "retention_score": record.retention_score,
            "counters": record.counters,
            "timestamp": record.timestamp,
            "version": record.version,
        }
        meta_bytes = json.dumps(meta, ensure_ascii=False).encode("utf-8")
        vector_len = int(record.vector.size)
        vector_bytes = record.vector.astype(np.float32, copy=False).tobytes()
        header = struct.pack("<I16sI", record.version, uuid.UUID(record.memory_id).bytes, len(meta_bytes))
        payload = meta_bytes + struct.pack("<I", vector_len) + vector_bytes
        return header + payload

    def _read_record(self, f) -> MemoryRecord:
        version_bytes = self._read_exact(f, 4)
        version = struct.unpack("<I", version_bytes)[0]
        if version != SCHEMA_VERSION:
            raise RuntimeError(
                f"Schema version mismatch in record: expected {SCHEMA_VERSION}, found {version}"
            )
        memory_id_bytes = self._read_exact(f, 16)
        meta_len = struct.unpack("<I", self._read_exact(f, 4))[0]
        meta_bytes = self._read_exact(f, meta_len)
        meta = json.loads(meta_bytes.decode("utf-8"))

        vector_len = struct.unpack("<I", self._read_exact(f, 4))[0]
        vector_bytes = self._read_exact(f, vector_len * 4)
        vector = np.frombuffer(vector_bytes, dtype=np.float32).copy()

        memory_id = str(uuid.UUID(bytes=memory_id_bytes))
        return MemoryRecord(
            memory_id=memory_id,
            vector=vector,
            store_type=StoreType(meta["store_type"]),
            status=MemoryStatus(meta["status"]),
            decision=Decision(meta["decision"]),
            confidence_init=float(meta["confidence_init"]),
            retention_score=float(meta["retention_score"]),
            counters=dict(meta.get("counters", {})),
            timestamp=int(meta["timestamp"]),
            version=version,
        )

    def _encode_index_record(self, offset: int, record: MemoryRecord) -> bytes:
        return struct.pack("<Q16sI", int(offset), uuid.UUID(record.memory_id).bytes, int(record.vector.size))

    @staticmethod
    def _read_exact(f, size: int) -> bytes:
        data = f.read(size)
        if len(data) != size:
            raise EOFError("Unexpected end of file while reading record")
        return data


class HolographicMemory:
    """判断付き記憶の遷移を扱うメモリ層。"""

    def __init__(self, root_dir: Path | str = "memory_state") -> None:
        root = Path(root_dir)
        self.canonical_store = FileMemoryStore(root / "canonical", StoreType.CANONICAL)
        self.quarantine_store = FileMemoryStore(root / "quarantine", StoreType.QUARANTINE)
        self.working_memory: List[MemoryRecord] = []
        self.logger = EventLogger(root / "logs" / "events.log")

    def load(self) -> None:
        self.canonical_store.load()
        self.quarantine_store.load()

    def flush(self) -> None:
        self.canonical_store.flush()
        self.quarantine_store.flush()

    def store_decision(
        self,
        vector: Sequence[float],
        decision: Decision,
        confidence_init: float,
        retention_score: float,
        counters: Dict[str, float],
        memory_id: Optional[str] = None,
        timestamp: Optional[int] = None,
    ) -> MemoryRecord:
        if memory_id is None:
            memory_id = str(uuid.uuid4())
        if timestamp is None:
            timestamp = int(time.time())

        if decision == Decision.ACCEPT:
            store = self.canonical_store
            store_type = StoreType.CANONICAL
        elif decision in (Decision.REVIEW, Decision.REJECT):
            store = self.quarantine_store
            store_type = StoreType.QUARANTINE
        else:
            raise ValueError(f"Unknown decision: {decision}")

        record = MemoryRecord(
            memory_id=memory_id,
            vector=np.asarray(vector, dtype=np.float32),
            store_type=store_type,
            status=MemoryStatus.ACTIVE,
            decision=decision,
            confidence_init=confidence_init,
            retention_score=retention_score,
            counters=dict(counters),
            timestamp=timestamp,
        )
        store.append(record)
        self.logger.write(
            "MEMORY_STORED",
            {
                "memory_id": record.memory_id,
                "store_type": record.store_type,
                "status": record.status,
                "decision": record.decision,
                "confidence_init": record.confidence_init,
                "retention_score": record.retention_score,
            },
        )
        return record

    def recall(self, query_vector: Sequence[float], k: int, context: RecallContext) -> List[RecallHit]:
        canonical_hits = self.canonical_store.recall(
            query_vector, k, include_status=[MemoryStatus.ACTIVE]
        )
        if canonical_hits:
            best = canonical_hits[0].resonance
        else:
            best = 0.0

        if best >= context.canonical_min_similarity and context.entropy < 0.5 and not context.expected_utility_improve:
            hits = canonical_hits
        else:
            quarantine_hits = self.quarantine_store.recall(
                query_vector, k, include_status=[MemoryStatus.ACTIVE]
            )
            hits = canonical_hits + quarantine_hits
            if context.allow_frozen:
                frozen_hits = self.quarantine_store.recall(
                    query_vector, k, include_status=[MemoryStatus.FROZEN]
                )
                hits.extend(frozen_hits)

        hits = sorted(hits, key=lambda h: h.resonance, reverse=True)[:k]
        for hit in hits:
            self.logger.write(
                "MEMORY_RECALLED",
                {
                    "memory_id": hit.memory_id,
                    "store_type": hit.store_type,
                    "status": hit.status,
                    "resonance": hit.resonance,
                },
            )
        return hits

    def promote_if_eligible(self, memory_id: str) -> Optional[MemoryRecord]:
        latest = self.quarantine_store.latest_state().get(memory_id)
        if latest is None:
            return None
        if not self._meets_promotion_conditions(latest):
            return None

        promoted = MemoryRecord(
            memory_id=latest.memory_id,
            vector=latest.vector,
            store_type=StoreType.CANONICAL,
            status=MemoryStatus.ACTIVE,
            decision=Decision.ACCEPT,
            confidence_init=latest.confidence_init,
            retention_score=latest.retention_score,
            counters=dict(latest.counters),
            timestamp=int(time.time()),
        )
        self.canonical_store.append(promoted)
        self.logger.write(
            "MEMORY_PROMOTED",
            {
                "memory_id": promoted.memory_id,
                "from_store": StoreType.QUARANTINE,
                "to_store": StoreType.CANONICAL,
                "status": promoted.status,
            },
        )
        return promoted

    def freeze(self, memory_id: str, retention_score: float, reason: str) -> Optional[MemoryRecord]:
        latest = self.quarantine_store.latest_state().get(memory_id)
        if latest is None:
            return None
        record = MemoryRecord(
            memory_id=latest.memory_id,
            vector=latest.vector,
            store_type=StoreType.QUARANTINE,
            status=MemoryStatus.FROZEN,
            decision=latest.decision,
            confidence_init=latest.confidence_init,
            retention_score=retention_score,
            counters=dict(latest.counters),
            timestamp=int(time.time()),
        )
        self.quarantine_store.append(record)
        self.logger.write(
            "MEMORY_STATUS_CHANGED",
            {
                "memory_id": record.memory_id,
                "store_type": record.store_type,
                "status": record.status,
                "reason": reason,
            },
        )
        return record

    def disable(self, memory_id: str, reason: str, human_override: bool = False) -> Optional[MemoryRecord]:
        latest = self.quarantine_store.latest_state().get(memory_id)
        if latest is None:
            latest = self.canonical_store.latest_state().get(memory_id)
        if latest is None:
            return None

        target_store = (
            self.quarantine_store if latest.store_type == StoreType.QUARANTINE else self.canonical_store
        )
        record = MemoryRecord(
            memory_id=latest.memory_id,
            vector=latest.vector,
            store_type=latest.store_type,
            status=MemoryStatus.DISABLED,
            decision=latest.decision,
            confidence_init=latest.confidence_init,
            retention_score=latest.retention_score,
            counters=dict(latest.counters),
            timestamp=int(time.time()),
        )
        target_store.append(record)

        event_type = "HUMAN_OVERRIDE" if human_override else "MEMORY_STATUS_CHANGED"
        self.logger.write(
            event_type,
            {
                "memory_id": record.memory_id,
                "store_type": record.store_type,
                "status": record.status,
                "reason": reason,
            },
        )
        return record

    def _meets_promotion_conditions(self, record: MemoryRecord) -> bool:
        counters = record.counters
        return (
            record.confidence_init >= 0.40
            and counters.get("reuse_count", 0) >= 2
            and counters.get("accept_support_count", 0) >= 1
            and counters.get("avg_EU_delta", 0.0) >= 0.05
            and counters.get("reject_impact_count", 0) == 0
        )
