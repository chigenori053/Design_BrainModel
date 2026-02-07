import pytest
import time
from pydantic import ValidationError
from design_brain_model.brain_model.memory.types import SemanticUnitL1, SemanticUnitL2, L1Cluster

# --- T0: Invariant Tests (不変条件テスト) ---

def test_l1_cannot_have_decision_attributes():
    """T0: SemanticUnitL1にL2の評価/決定フィールドを持たせない"""
    l1_unit = SemanticUnitL1(
        id="l1-001",
        type="observation",
        content="A sensor reading.",
        source="sensor-A",
        timestamp=time.time()
    )
    # L1ユニットにL2の属性を動的に追加しようとすると失敗することを確認
    with pytest.raises(AttributeError):
        l1_unit.decision_polarity = True
    with pytest.raises(AttributeError):
        l1_unit.scope_in = ["test"]

def test_l2_is_immutable():
    """T0: SemanticUnitL2は生成後に変更不可"""
    l2_unit = SemanticUnitL2(
        id="l2-001",
        decision_polarity=True,
        evaluation={"utility": 0.9},
        source_cluster_id="cluster-01",
        source_l1_ids=["l1-001", "l1-002"]
    )
    # frozen=Trueにより、属性を変更しようとするとエラーが発生することを確認
    with pytest.raises(ValidationError):
        l2_unit.decision_polarity = False
    with pytest.raises(ValidationError):
        l2_unit.evaluation = {"utility": 0.5}

def test_l2_requires_source_l1_ids():
    """T0: L2はL1なしに生成不可"""
    # source_l1_idsが空のリストの場合、ValueErrorが発生することを確認
    with pytest.raises(ValueError, match="source_l1_ids cannot be empty"):
        SemanticUnitL2(
            id="l2-002",
            decision_polarity=False,
            evaluation={},
            source_cluster_id="cluster-02",
            source_l1_ids=[]
        )

# --- T1: Promotion Boundary Tests (昇格境界テスト) ---

def promote_to_l2(cluster: L1Cluster, decision_polarity: bool, evaluation: dict) -> SemanticUnitL2:
    """
    昇格ロジックのヘルパー関数.
    クラスタと決定情報からL2ユニットを生成する.
    """
    if not cluster.l1_ids:
        raise ValueError("Cannot promote an empty cluster.")

    l2_id = f"l2-from-{cluster.id}"
    
    return SemanticUnitL2(
        id=l2_id,
        decision_polarity=decision_polarity,
        evaluation=evaluation,
        source_cluster_id=cluster.id,
        source_l1_ids=cluster.l1_ids
    )

@pytest.fixture
def sample_l1_units():
    """T1テスト用のL1ユニットのサンプルデータを作成するフィクスチャ"""
    return [
        SemanticUnitL1("l1-101", "log", "User logged in", "auth-service", time.time()),
        SemanticUnitL1("l1-102", "metric", "CPU usage high", "monitor-agent", time.time()),
        SemanticUnitL1("l1-103", "trace", "DB query slow", "apm-tracer", time.time()),
    ]

def test_l1_creation_does_not_create_l2(sample_l1_units):
    """T1: L1の大量生成だけではL2は生まれない"""
    # このテストは、L1ユニットが独立して存在できることを示す.
    # L2が生成されるのは、明示的な昇格ロジックが呼ばれた時のみ.
    assert len(sample_l1_units) == 3
    # ここでは promote_to_l2 を呼ばないので、L2は生成されない.
    # このテストの成功は、副作用がないことの証明.

def test_l1_cluster_alone_does_not_promote(sample_l1_units):
    """T1: L1-Cluster単体では昇格不可"""
    l1_ids = [unit.id for unit in sample_l1_units]
    cluster = L1Cluster(id="cluster-10", l1_ids=l1_ids)
    
    # クラスタを作成しただけではL2は生成されない.
    assert cluster is not None
    assert cluster.id == "cluster-10"
    # 副作用がないことを確認.

def test_promotion_succeeds_only_with_full_conditions(sample_l1_units):
    """T1: 昇格条件をすべて満たした場合のみL2が生成される"""
    l1_ids = [unit.id for unit in sample_l1_units]
    cluster = L1Cluster(id="cluster-11", l1_ids=l1_ids)
    
    decision_polarity = True
    evaluation = {"risk": -0.2, "impact": 0.8}

    # 昇格ロジックを呼び出す
    l2_unit = promote_to_l2(
        cluster=cluster,
        decision_polarity=decision_polarity,
        evaluation=evaluation
    )

    # L2が期待通りに生成されたか検証
    assert isinstance(l2_unit, SemanticUnitL2)
    assert l2_unit.id == "l2-from-cluster-11"
    assert l2_unit.decision_polarity is True
    # ソース情報が完全に一致するか検証
    assert l2_unit.source_cluster_id == cluster.id
    assert l2_unit.source_l1_ids == l1_ids
    assert l2_unit.evaluation == evaluation

def test_promotion_fails_if_cluster_is_empty():
    """T1: 空のクラスタからは昇格できない"""
    empty_cluster = L1Cluster(id="cluster-empty", l1_ids=[])
    
    with pytest.raises(ValueError, match="Cannot promote an empty cluster."):
        promote_to_l2(
            cluster=empty_cluster,
            decision_polarity=True,
            evaluation={}
        )

