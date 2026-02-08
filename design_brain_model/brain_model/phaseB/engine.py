from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Protocol
import math

from design_brain_model.brain_model.phaseA.math_core import AlgorithmicMathCore
from .types import (
    EVALUATION_AXES,
    EvaluationAxis,
    EvaluationReport,
    EvaluationTarget,
    GeometryPoint,
)


@dataclass(frozen=True)
class MetricVector:
    scores: Dict[EvaluationAxis, float]

class MathCoreProtocol(Protocol):
    def calculate_scores(self, structure: Dict[str, object]) -> Dict[str, float]:
        ...


class MetricCalculator:
    """
    PhaseB: 評価軸ごとのスコアを算出する。
    PhaseA Math Core のみを使用する。
    """
    def __init__(self, math_core: MathCoreProtocol):
        self._math_core = math_core

    def calculate(self, target: EvaluationTarget) -> MetricVector:
        raw_scores = self._math_core.calculate_scores(target.structure)
        scores = {}
        for axis in EVALUATION_AXES:
            key = axis.value
            if key not in raw_scores:
                raise ValueError(f"Math Core のスコアに軸が不足しています: {key}")
            scores[axis] = float(raw_scores[key])
        return MetricVector(scores=scores)


class GeometryMapper:
    """
    スコア → GeometryPoint に変換する。
    """
    def map(self, target: EvaluationTarget, scores: Dict[EvaluationAxis, float]) -> GeometryPoint:
        vector = [float(scores[axis]) for axis in EVALUATION_AXES]
        return GeometryPoint(vector=vector, source_id=target.id)


class DistanceEvaluator:
    """
    ユークリッド距離（固定）。
    """
    def distance(self, p1: GeometryPoint, p2: GeometryPoint) -> float:
        if len(p1.vector) != len(p2.vector):
            raise ValueError("GeometryPoint 次元数が一致しません。")
        return math.sqrt(sum((a - b) ** 2 for a, b in zip(p1.vector, p2.vector)))


class EvaluationReportBuilder:
    """
    数値を人間可読構造に変換する。結論や推薦は禁止。
    """
    def build(
        self,
        targets: List[EvaluationTarget],
        points: List[GeometryPoint],
        distances: List[List[float]],
        qualitative_notes: str | None = None
    ) -> EvaluationReport:
        return EvaluationReport(
            targets=[t.id for t in targets],
            geometry_points=points,
            distances=distances,
            qualitative_notes=qualitative_notes,
        )


class EvaluationEngine:
    """
    PhaseB 評価エンジン。
    対象を同一評価空間へ写像し、距離行列を生成する。
    """
    def __init__(self, math_core: AlgorithmicMathCore | None = None):
        self._math_core = math_core or AlgorithmicMathCore()
        self.metric_calculator = MetricCalculator(self._math_core)
        self.geometry_mapper = GeometryMapper()
        self.distance_evaluator = DistanceEvaluator()
        self.report_builder = EvaluationReportBuilder()

    def evaluate(self, targets: List[EvaluationTarget]) -> EvaluationReport:
        if not targets:
            raise ValueError("評価対象が空です。")
        for target in targets:
            if not isinstance(target, EvaluationTarget):
                raise TypeError("EvaluationTarget 以外は評価対象として受理できません。")

        raw_metrics = [self.metric_calculator.calculate(t) for t in targets]
        normalized = _normalize_metrics(raw_metrics)
        points = [
            self.geometry_mapper.map(targets[i], normalized[i].scores)
            for i in range(len(targets))
        ]
        distances = _pairwise_distances(points, self.distance_evaluator)
        return self.report_builder.build(targets, points, distances)


def _pairwise_distances(points: List[GeometryPoint], evaluator: DistanceEvaluator) -> List[List[float]]:
    n = len(points)
    matrix: List[List[float]] = []
    for i in range(n):
        row = []
        for j in range(n):
            if i == j:
                row.append(0.0)
            else:
                row.append(evaluator.distance(points[i], points[j]))
        matrix.append(row)
    return matrix


def _normalize_metrics(metrics: List[MetricVector]) -> List[MetricVector]:
    # 相対値のみを扱うため、各軸で min-max 正規化
    mins: Dict[EvaluationAxis, float] = {axis: float("inf") for axis in EVALUATION_AXES}
    maxs: Dict[EvaluationAxis, float] = {axis: float("-inf") for axis in EVALUATION_AXES}

    for mv in metrics:
        for axis in EVALUATION_AXES:
            val = mv.scores[axis]
            mins[axis] = min(mins[axis], val)
            maxs[axis] = max(maxs[axis], val)

    normalized: List[MetricVector] = []
    for mv in metrics:
        scores: Dict[EvaluationAxis, float] = {}
        for axis in EVALUATION_AXES:
            low = mins[axis]
            high = maxs[axis]
            val = mv.scores[axis]
            if high == low:
                scores[axis] = 0.0
            else:
                scores[axis] = (val - low) / (high - low)
        normalized.append(MetricVector(scores=scores))
    return normalized

