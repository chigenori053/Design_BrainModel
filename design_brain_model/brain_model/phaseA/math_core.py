from __future__ import annotations

from typing import Any, Dict, List


AXIS_NAMES: List[str] = [
    "Structural Consistency",
    "Reusability",
    "Complexity",
    "Constraint Satisfaction",
    "Clarity",
]


class AlgorithmicMathCore:
    """
    PhaseA: Algorithmic Math Core (最小実装)
    PhaseB はこの Math Core のみを使用して評価軸スコアを算出する。
    """
    def calculate_scores(self, structure: Dict[str, Any]) -> Dict[str, float]:
        key_count = len(structure.keys())
        leaf_count = _count_leaves(structure)
        text_len = _estimate_text_length(structure)
        constraint_count = _count_constraints(structure)

        return {
            "Structural Consistency": _safe_ratio(key_count, key_count + leaf_count),
            "Reusability": _safe_ratio(leaf_count, max(1, key_count)),
            "Complexity": float(key_count + leaf_count),
            "Constraint Satisfaction": _safe_ratio(constraint_count, max(1, key_count)),
            "Clarity": _safe_ratio(1.0, max(1.0, text_len)),
        }


def _count_leaves(value: object) -> int:
    if value is None:
        return 0
    if isinstance(value, dict):
        return sum(_count_leaves(v) for v in value.values())
    if isinstance(value, (list, tuple)):
        return sum(_count_leaves(v) for v in value)
    return 1


def _estimate_text_length(value: object) -> float:
    if value is None:
        return 0.0
    if isinstance(value, dict):
        return sum(_estimate_text_length(v) for v in value.values())
    if isinstance(value, (list, tuple)):
        return sum(_estimate_text_length(v) for v in value)
    if isinstance(value, str):
        return float(len(value))
    return 1.0


def _count_constraints(value: object) -> int:
    if value is None:
        return 0
    if isinstance(value, dict):
        count = 0
        for k, v in value.items():
            if "constraint" in str(k).lower():
                count += 1
            count += _count_constraints(v)
        return count
    if isinstance(value, (list, tuple)):
        return sum(_count_constraints(v) for v in value)
    return 0


def _safe_ratio(numerator: float, denominator: float) -> float:
    if denominator == 0:
        return 0.0
    return float(numerator) / float(denominator)

