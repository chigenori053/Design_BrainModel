from pathlib import Path

import pytest

from design_brain_model.brain_model.knowledge import (
    KnowledgeStore,
    KnowledgeStoreInput,
    KnowledgeUnit,
    KnowledgeOrigin,
    OriginSourceType,
    KnowledgeType,
    Scope,
)


def _sample_unit(structure: dict, source_type: OriginSourceType = OriginSourceType.HUMAN) -> KnowledgeUnit:
    origin = KnowledgeOrigin(source_type=source_type, evidence_id="ev-1" if source_type != OriginSourceType.HUMAN else None)
    return KnowledgeUnit(
        type=KnowledgeType.STRUCTURAL,
        abstract_structure=structure,
        constraints=[],
        applicability_scope=Scope(domain="ui", conditions={"platform": "web"}),
        origin=origin,
        confidence=None,
    )


def test_store_and_get_knowledge(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    unit = _sample_unit({"nodes": ["A", "B"], "edges": [["A", "B"]]})

    knowledge_id = store.store_knowledge(
        KnowledgeStoreInput(
            knowledge=unit,
            human_override=True,
            reusability_confirmed=True,
        )
    )
    fetched = store.get_knowledge(knowledge_id)

    assert fetched is not None
    assert fetched.id == unit.id
    assert fetched.abstract_structure == unit.abstract_structure


def test_store_requires_human_override(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    unit = _sample_unit({"nodes": ["A"]})

    with pytest.raises(ValueError, match="Human Override"):
        store.store_knowledge(
            KnowledgeStoreInput(
                knowledge=unit,
                human_override=False,
                reusability_confirmed=True,
            )
        )


def test_origin_evidence_id_required_for_web_doc(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="evidence_id"):
        KnowledgeOrigin(source_type=OriginSourceType.WEB, evidence_id=None)

    store = KnowledgeStore(tmp_path)
    unit = _sample_unit({"nodes": ["A"]}, source_type=OriginSourceType.DOC)
    store.store_knowledge(
        KnowledgeStoreInput(
            knowledge=unit,
            human_override=True,
            reusability_confirmed=True,
        )
    )


def test_recall_prefers_structure_similarity(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    unit_a = _sample_unit({"nodes": ["A", "B"], "edges": [["A", "B"]]})
    unit_b = _sample_unit({"nodes": ["X", "Y"], "edges": [["X", "Y"]]})

    store.store_knowledge(KnowledgeStoreInput(knowledge=unit_a, human_override=True, reusability_confirmed=True))
    store.store_knowledge(KnowledgeStoreInput(knowledge=unit_b, human_override=True, reusability_confirmed=True))

    hits = store.recall_knowledge({"nodes": ["A"], "edges": [["A", "B"]]})
    assert hits
    assert hits[0].knowledge_id == unit_a.id
    assert hits[0].similarity > 0.0

