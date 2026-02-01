from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Protocol, Sequence, Tuple
import json
import os
import struct
import uuid

import numpy as np

from .types import MemoryStatus

SCHEMA_VERSION = 1


@dataclass(slots=True)
class HolographicTrace:
    trace_id: str
    source_unit_id: str
    raw_vector: np.ndarray
    interference_vector: Optional[np.ndarray]
    energy: float
    timestamp: int
    status: MemoryStatus = MemoryStatus.ACTIVE
    version: int = SCHEMA_VERSION

    def __post_init__(self) -> None:
        self.trace_id = str(self.trace_id) if self.trace_id else str(uuid.uuid4())
        if not self.source_unit_id:
            raise ValueError("source_unit_id is required")
        self.raw_vector = np.asarray(self.raw_vector, dtype=np.float32)
        if self.raw_vector.ndim != 1 or self.raw_vector.size == 0:
            raise ValueError("raw_vector must be a non-empty 1D float32 array")
        if self.interference_vector is not None:
            self.interference_vector = np.asarray(self.interference_vector, dtype=np.float32)
            if self.interference_vector.ndim != 1:
                raise ValueError("interference_vector must be a 1D float32 array")
        self.energy = float(self.energy)
        self.timestamp = int(self.timestamp)
        self.status = MemoryStatus(self.status) if isinstance(self.status, str) else self.status
        self.version = int(self.version)


@dataclass(slots=True)
class RecallResult:
    trace_id: str
    source_unit_id: str
    resonance: float
    energy: float
    timestamp: int


class HolographicStore(Protocol):
    def append(self, trace: HolographicTrace) -> None:
        ...

    def recall(self, query_vector: Sequence[float], k: int) -> List[RecallResult]:
        ...

    def load(self) -> None:
        ...

    def flush(self) -> None:
        ...

    def stats(self) -> Dict[str, object]:
        ...


class FileHolographicStore:
    """
    Append-only persistent store for HolographicTrace.
    This is not a CRUD database; it is a recall-optimized trace log.

    NOTE:
    index.bin is currently not used.
    It will be regenerated when ANN / index-based recall is introduced.
    Currently, _rewrite_all() (used for status updates) does NOT update index.bin.
    """

    TRACE_FILENAME = "traces.bin"
    INDEX_FILENAME = "index.bin"
    META_FILENAME = "meta.json"

    def __init__(self, store_dir: Path | str = "memory_store") -> None:
        self.store_dir = Path(store_dir)
        self.store_dir.mkdir(parents=True, exist_ok=True)
        self.traces_path = self.store_dir / self.TRACE_FILENAME
        self.index_path = self.store_dir / self.INDEX_FILENAME
        self.meta_path = self.store_dir / self.META_FILENAME

        self._traces: List[HolographicTrace] = []
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

        if not self.traces_path.exists():
            self._loaded = True
            return

        self._traces = []
        self._offsets = []
        self._vector_dim = None

        with self.traces_path.open("rb") as f:
            file_size = os.fstat(f.fileno()).st_size
            while f.tell() < file_size:
                offset = f.tell()
                trace = self._read_trace(f)
                self._offsets.append(offset)
                self._traces.append(trace)
                if self._vector_dim is None:
                    self._vector_dim = int(trace.raw_vector.size)

        self._loaded = True

    def append(self, trace: HolographicTrace) -> None:
        if not self._loaded:
            self.load()
        if trace.version != SCHEMA_VERSION:
            raise ValueError("Trace version mismatch with store schema")
        if self._vector_dim is None:
            self._vector_dim = int(trace.raw_vector.size)
        elif self._vector_dim != int(trace.raw_vector.size):
            raise ValueError("raw_vector dimension does not match existing store")

        with self.traces_path.open("ab") as f:
            offset = f.tell()
            record = self._encode_trace(trace)
            f.write(record)
            f.flush()

        with self.index_path.open("ab") as index_f:
            index_f.write(self._encode_index_record(offset, trace))
            index_f.flush()

        self._offsets.append(offset)
        self._traces.append(trace)

    def recall(
        self,
        query_vector: Sequence[float],
        k: int,
        include_statuses: Optional[Set[MemoryStatus]] = None
    ) -> List[RecallResult]:
        if not self._loaded:
            self.load()
        if k <= 0:
            return []

        if include_statuses is None:
            # Default to only ACTIVE as per Spec-02 for normal recall
            include_statuses = {MemoryStatus.ACTIVE}

        query = np.asarray(query_vector, dtype=np.float32)
        if query.ndim != 1 or query.size == 0:
            raise ValueError("query_vector must be a non-empty 1D float32 array")
        if self._vector_dim is not None and query.size != self._vector_dim:
            raise ValueError("query_vector dimension does not match store")

        query_norm = np.linalg.norm(query)
        if query_norm == 0:
            return []
        query = query / query_norm

        scores: List[Tuple[int, float]] = []
        for idx, trace in enumerate(self._traces):
            if trace.status not in include_statuses:
                continue

            vector = trace.raw_vector
            if trace.interference_vector is not None and trace.interference_vector.size == vector.size:
                vector = vector + trace.interference_vector
            vec_norm = np.linalg.norm(vector)
            if vec_norm == 0:
                continue
            resonance = float(np.dot(query, vector / vec_norm))
            if resonance > 0:
                scores.append((idx, resonance))

        scores.sort(key=lambda item: item[1], reverse=True)
        results: List[RecallResult] = []
        for idx, resonance in scores[:k]:
            trace = self._traces[idx]
            results.append(
                RecallResult(
                    trace_id=trace.trace_id,
                    source_unit_id=trace.source_unit_id,
                    resonance=resonance,
                    energy=trace.energy,
                    timestamp=trace.timestamp,
                )
            )
        return results

    def update_status(self, trace_id: str, new_status: MemoryStatus) -> bool:
        """Updates the status of a trace and persists the change."""
        if not self._loaded:
            self.load()

        found = False
        for trace in self._traces:
            if trace.trace_id == trace_id:
                trace.status = new_status
                found = True
                break
        
        if found:
            # For simplicity in Phase 19, we rewrite the entire file to reflect the change.
            # In a production append-only store, we'd append a 'status change' record.
            self._rewrite_all()
            return True
        return False

    def get_trace_by_source_unit_id(self, source_unit_id: str) -> Optional[HolographicTrace]:
        """Spec-04: Safely retrieve a trace by its source unit ID, ensuring load() is called."""
        if not self._loaded:
            self.load()
        
        # Search for the latest trace associated with this source_unit_id
        for trace in reversed(self._traces):
            if trace.source_unit_id == source_unit_id:
                return trace
        return None

    def _rewrite_all(self) -> None:
        """Rewrites the entire traces file from the current in-memory traces."""
        with self.traces_path.open("wb") as f:
            for trace in self._traces:
                f.write(self._encode_trace(trace))
        self.flush()

    def flush(self) -> None:
        if not self._loaded:
            return
        meta = {
            "schema_version": SCHEMA_VERSION,
            "trace_count": len(self._traces),
            "vector_dim": self._vector_dim,
            "last_timestamp": self._traces[-1].timestamp if self._traces else None,
        }
        self.meta_path.write_text(json.dumps(meta, indent=2), encoding="utf-8")

    def stats(self) -> Dict[str, object]:
        return {
            "schema_version": SCHEMA_VERSION,
            "trace_count": len(self._traces),
            "vector_dim": self._vector_dim,
            "store_dir": str(self.store_dir),
        }

    def _encode_trace(self, trace: HolographicTrace) -> bytes:
        source_bytes = trace.source_unit_id.encode("utf-8")
        raw_len = int(trace.raw_vector.size)
        raw_bytes = trace.raw_vector.astype(np.float32, copy=False).tobytes()

        has_interference = trace.interference_vector is not None
        interference_bytes = b""
        interference_len = 0
        if has_interference:
            interference = trace.interference_vector.astype(np.float32, copy=False)
            interference_len = int(interference.size)
            interference_bytes = interference.tobytes()

        status_map = {MemoryStatus.ACTIVE: 0, MemoryStatus.FROZEN: 1, MemoryStatus.DISABLED: 2}
        status_val = status_map.get(trace.status, 0)

        header = struct.pack(
            "<I16sII",
            trace.version,
            uuid.UUID(trace.trace_id).bytes,
            len(source_bytes),
            status_val,
        )
        payload = struct.pack("<I", raw_len) + raw_bytes
        payload += struct.pack("<B", 1 if has_interference else 0)
        if has_interference:
            payload += struct.pack("<I", interference_len) + interference_bytes
        tail = struct.pack("<fq", float(trace.energy), int(trace.timestamp))
        return header + source_bytes + payload + tail

    def _read_trace(self, f) -> HolographicTrace:
        version_bytes = self._read_exact(f, 4)
        if not version_bytes:
            raise EOFError("Unexpected end of file while reading trace")
        version = struct.unpack("<I", version_bytes)[0]
        if version != SCHEMA_VERSION:
            raise RuntimeError(
                f"Schema version mismatch in trace: expected {SCHEMA_VERSION}, found {version}"
            )
        trace_id_bytes = self._read_exact(f, 16)
        header_remaining = struct.unpack("<II", self._read_exact(f, 8))
        source_len = header_remaining[0]
        status_val = header_remaining[1]
        
        source_unit_id = self._read_exact(f, source_len).decode("utf-8")

        raw_len = struct.unpack("<I", self._read_exact(f, 4))[0]
        raw_bytes = self._read_exact(f, raw_len * 4)
        raw_vector = np.frombuffer(raw_bytes, dtype=np.float32).copy()

        has_interference = struct.unpack("<B", self._read_exact(f, 1))[0]
        interference_vector = None
        if has_interference:
            interference_len = struct.unpack("<I", self._read_exact(f, 4))[0]
            interference_bytes = self._read_exact(f, interference_len * 4)
            interference_vector = np.frombuffer(interference_bytes, dtype=np.float32).copy()

        energy = struct.unpack("<f", self._read_exact(f, 4))[0]
        timestamp = struct.unpack("<q", self._read_exact(f, 8))[0]

        status_rev_map = {0: MemoryStatus.ACTIVE, 1: MemoryStatus.FROZEN, 2: MemoryStatus.DISABLED}
        status = status_rev_map.get(status_val, MemoryStatus.ACTIVE)

        trace_id = str(uuid.UUID(bytes=trace_id_bytes))
        return HolographicTrace(
            trace_id=trace_id,
            source_unit_id=source_unit_id,
            raw_vector=raw_vector,
            interference_vector=interference_vector,
            energy=energy,
            timestamp=timestamp,
            status=status,
            version=version,
        )

    def _encode_index_record(self, offset: int, trace: HolographicTrace) -> bytes:
        return struct.pack(
            "<Q16sI",
            int(offset),
            uuid.UUID(trace.trace_id).bytes,
            int(trace.raw_vector.size),
        )

    @staticmethod
    def _read_exact(f, size: int) -> bytes:
        data = f.read(size)
        if len(data) != size:
            raise EOFError("Unexpected end of file while reading trace")
        return data
