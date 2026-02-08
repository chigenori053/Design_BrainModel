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
from design_brain_model.brain_model.phaseA import (
    DesignAgent,
    EvidenceStore,
    AlgorithmicMathCore,
)
from design_brain_model.brain_model.phaseB import (
    EvaluationEngine,
    EvaluationTarget,
    EvaluationTargetType,
)


def _sample_knowledge_unit(structure: dict) -> KnowledgeUnit:
    return KnowledgeUnit(
        type=KnowledgeType.STRUCTURAL,
        abstract_structure=structure,
        constraints=[],
        applicability_scope=Scope(domain="ui", conditions={"platform": "web"}),
        origin=KnowledgeOrigin(source_type=OriginSourceType.HUMAN),
    )


def _store_knowledge(store: KnowledgeStore, unit: KnowledgeUnit) -> str:
    return store.store_knowledge(
        KnowledgeStoreInput(
            knowledge=unit,
            human_override=True,
            reusability_confirmed=True,
        )
    )


def test_tc_a1_knowledge_to_evaluation_flow(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    agent = DesignAgent()
    engine = EvaluationEngine()

    unit = _sample_knowledge_unit({"nodes": ["A", "B"], "edges": [["A", "B"]]})
    _store_knowledge(store, unit)

    design = agent.generate_design_from_knowledge(unit)
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure=design)

    report = engine.evaluate([target])
    assert report.geometry_points
    assert store.get_knowledge(unit.id) is not None

    with pytest.raises(TypeError):
        engine.evaluate([unit])  # KnowledgeUnit の直接評価は禁止


def test_tc_a2_recall_isolated_from_evaluation(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    agent = DesignAgent()
    engine = EvaluationEngine()

    unit = _sample_knowledge_unit({"nodes": ["A", "B"], "edges": [["A", "B"]]})
    _store_knowledge(store, unit)

    def _raise(*_args, **_kwargs):
        raise AssertionError("Recall が評価に介入しました。")

    store.recall_knowledge = _raise  # type: ignore[assignment]

    design = agent.generate_design_from_knowledge(unit)
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure=design)

    report1 = engine.evaluate([target])
    report2 = engine.evaluate([target])

    assert report1.geometry_points[0].vector == report2.geometry_points[0].vector
    assert report1.distances[0][0] == 0.0
    assert report2.distances[0][0] == 0.0


def test_tc_b1_evidence_is_not_an_evaluation_target() -> None:
    store = EvidenceStore()
    evidence_id = store.store_evidence(summary="web result", source="web")

    engine = EvaluationEngine()
    with pytest.raises(TypeError):
        engine.evaluate([evidence_id])  # Evidence ID は評価対象にならない


def test_tc_b2_evidence_to_design_to_evaluation(tmp_path: Path) -> None:
    evidence_store = EvidenceStore()
    knowledge_store = KnowledgeStore(tmp_path)
    agent = DesignAgent()
    engine = EvaluationEngine()

    evidence_id = evidence_store.store_evidence(summary="web result", source="web")
    record = evidence_store.get(evidence_id)
    assert record is not None

    design = agent.generate_design_from_evidence(record.__dict__)
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure=design)

    before_evidence = evidence_store.snapshot()
    report = engine.evaluate([target])
    after_evidence = evidence_store.snapshot()

    assert report.geometry_points
    assert before_evidence == after_evidence
    assert knowledge_store.get_knowledge("non-existent") is None


def test_tc_c1_math_core_is_used() -> None:
    class MockMathCore:
        def __init__(self):
            self.calls = 0

        def calculate_scores(self, structure: dict) -> dict:
            self.calls += 1
            return {
                "Structural Consistency": 1.0,
                "Reusability": 0.5,
                "Complexity": 2.0,
                "Constraint Satisfaction": 0.0,
                "Clarity": 0.8,
            }

    math_core = MockMathCore()
    engine = EvaluationEngine(math_core=math_core)
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure={"x": 1})

    engine.evaluate([target])
    assert math_core.calls == 1


def test_tc_d1_geometry_is_deterministic() -> None:
    engine = EvaluationEngine()
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure={"nodes": ["A"]})

    report1 = engine.evaluate([target])
    report2 = engine.evaluate([target])

    assert report1.geometry_points[0].vector == report2.geometry_points[0].vector
    assert report1.distances[0][0] == 0.0
    assert report2.distances[0][0] == 0.0


def test_tc_d2_distance_separates_different_designs() -> None:
    engine = EvaluationEngine()
    target_a = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure={"nodes": ["A"]})
    target_b = EvaluationTarget(
        type=EvaluationTargetType.DESIGN,
        structure={"nodes": ["A", "B", "C"], "edges": [["A", "B"], ["B", "C"]]},
    )

    report = engine.evaluate([target_a, target_b])
    assert report.distances[0][1] > 0.0
    assert report.distances[1][0] > 0.0


def test_tc_e1_phaseb_does_not_mutate_stores(tmp_path: Path) -> None:
    knowledge_store = KnowledgeStore(tmp_path)
    evidence_store = EvidenceStore()
    agent = DesignAgent()
    engine = EvaluationEngine()

    unit = _sample_knowledge_unit({"nodes": ["A", "B"], "edges": [["A", "B"]]})
    _store_knowledge(knowledge_store, unit)
    evidence_id = evidence_store.store_evidence(summary="web result", source="web")
    record = evidence_store.get(evidence_id)
    assert record is not None

    before_knowledge = knowledge_store.get_knowledge(unit.id).model_dump()
    before_evidence = evidence_store.snapshot()

    design = agent.generate_design_from_knowledge(unit)
    target = EvaluationTarget(type=EvaluationTargetType.DESIGN, structure=design)
    engine.evaluate([target])

    after_knowledge = knowledge_store.get_knowledge(unit.id).model_dump()
    after_evidence = evidence_store.snapshot()

    assert before_knowledge == after_knowledge
    assert before_evidence == after_evidence


def test_tc_e2_prohibited_actions_fail_fast(tmp_path: Path) -> None:
    store = KnowledgeStore(tmp_path)
    unit = _sample_knowledge_unit({"nodes": ["A"]})
    _store_knowledge(store, unit)

    engine = EvaluationEngine()
    with pytest.raises(TypeError):
        engine.evaluate([unit])

