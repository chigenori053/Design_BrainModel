import json
import uuid

import numpy as np
import pytest

from design_brain_model.brain_model.memory.persistent_store import (
    FileHolographicStore,
    HolographicTrace,
    SCHEMA_VERSION,
)


def _make_trace(vector, source_unit_id="unit-1"):
    return HolographicTrace(
        trace_id=str(uuid.uuid4()),
        source_unit_id=source_unit_id,
        raw_vector=np.asarray(vector, dtype=np.float32),
        interference_vector=None,
        energy=0.75,
        timestamp=1700000000,
        version=SCHEMA_VERSION,
    )


def test_append_load_recall_roundtrip(tmp_path):
    store = FileHolographicStore(tmp_path / "memory_store")
    vector = np.array([1.0, 0.0, 0.0], dtype=np.float32)
    trace = _make_trace(vector)

    store.append(trace)
    store.flush()

    reloaded = FileHolographicStore(tmp_path / "memory_store")
    reloaded.load()
    results = reloaded.recall(vector, k=1)

    assert len(results) == 1
    assert results[0].trace_id == trace.trace_id
    assert results[0].source_unit_id == trace.source_unit_id
    assert results[0].resonance == pytest.approx(1.0)


def test_large_append_recall_stability(tmp_path):
    store = FileHolographicStore(tmp_path / "memory_store")
    for i in range(200):
        vec = np.zeros(8, dtype=np.float32)
        vec[i % 8] = 1.0
        store.append(_make_trace(vec, source_unit_id=f"unit-{i}"))

    store.flush()

    query = np.zeros(8, dtype=np.float32)
    query[3] = 1.0
    results = store.recall(query, k=5)

    assert results
    assert results[0].resonance == pytest.approx(1.0)


def test_version_mismatch_blocks_load(tmp_path):
    store = FileHolographicStore(tmp_path / "memory_store")
    store.append(_make_trace([1.0, 0.0]))
    store.flush()

    meta_path = tmp_path / "memory_store" / "meta.json"
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    meta["schema_version"] = SCHEMA_VERSION + 1
    meta_path.write_text(json.dumps(meta), encoding="utf-8")

    reloaded = FileHolographicStore(tmp_path / "memory_store")
    with pytest.raises(RuntimeError):
        reloaded.load()
