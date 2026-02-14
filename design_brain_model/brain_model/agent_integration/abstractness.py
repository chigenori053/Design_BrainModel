from __future__ import annotations

import re
from typing import List, Tuple

from .types import DeepeningLevel, FramingFeedbackUnit, SemanticUnit, SemanticUnitKind, new_id


class AbstractnessAnalyzer:
    """
    Scores abstractness and produces framing feedback for high-abstract inputs.
    """

    _target_user_terms = [
        "user",
        "users",
        "customer",
        "customers",
        "audience",
        "client",
        "stakeholder",
        "persona",
        "ユーザー",
        "利用者",
        "顧客",
        "対象者",
        "対象",
    ]
    _metric_terms = [
        "kpi",
        "metric",
        "metrics",
        "latency",
        "throughput",
        "accuracy",
        "precision",
        "recall",
        "sla",
        "slo",
        "response time",
        "uptime",
        "availability",
        "秒",
        "ms",
        "%",
        "成功",
        "基準",
        "指標",
    ]

    def analyze(self, text: str, semantic_units: List[SemanticUnit]) -> FramingFeedbackUnit:
        missing_elements: List[str] = []
        score = 0

        has_objective = any(u.kind == SemanticUnitKind.OBJECTIVE for u in semantic_units)
        if not has_objective:
            score += 20
            missing_elements.append("objective")

        has_scope = any(u.kind == SemanticUnitKind.SCOPE for u in semantic_units)
        if not has_scope:
            score += 20
            missing_elements.append("scope")

        has_constraint = any(u.kind == SemanticUnitKind.CONSTRAINT for u in semantic_units)
        if not has_constraint:
            score += 20
            missing_elements.append("constraint")

        if not self._has_target_user(text):
            score += 20
            missing_elements.append("target_user")

        if not self._has_measurable_indicator(text):
            score += 20
            missing_elements.append("measurable_indicator")

        level = self._derive_level(score, has_objective, has_scope, has_constraint)
        questions = self._questions_for_missing(missing_elements)
        explanation = self._build_explanation(score, missing_elements, level)

        return FramingFeedbackUnit(
            id=new_id("frame"),
            abstract_score=score,
            missing_elements=missing_elements,
            clarification_questions=questions,
            explanation=explanation,
            deepening_level=level,
        )

    @staticmethod
    def _has_target_user(text: str) -> bool:
        lower = text.lower()
        return any(term in lower for term in AbstractnessAnalyzer._target_user_terms)

    @staticmethod
    def _has_measurable_indicator(text: str) -> bool:
        lower = text.lower()
        if re.search(r"\\b\\d+(\\.\\d+)?\\s*(ms|s|sec|seconds|%|％)\\b", lower):
            return True
        return any(term in lower for term in AbstractnessAnalyzer._metric_terms)

    @staticmethod
    def _derive_level(
        score: int,
        has_objective: bool,
        has_scope: bool,
        has_constraint: bool,
    ) -> DeepeningLevel:
        if score >= 60:
            return DeepeningLevel.VISION
        if has_objective and (has_scope or has_constraint):
            return DeepeningLevel.DESIGNABLE_STRUCTURE
        return DeepeningLevel.FRAMED_CONCEPT

    @staticmethod
    def _questions_for_missing(missing: List[str]) -> List[str]:
        questions = []
        if "objective" in missing:
            questions.append("What is the primary objective you want to achieve?")
        if "scope" in missing:
            questions.append("What is explicitly in scope and out of scope?")
        if "constraint" in missing:
            questions.append("What constraints or non-negotiables exist?")
        if "target_user" in missing:
            questions.append("Who is the target user or stakeholder?")
        if "measurable_indicator" in missing:
            questions.append("What measurable indicators define success?")
        return questions

    @staticmethod
    def _build_explanation(score: int, missing: List[str], level: DeepeningLevel) -> str:
        if score >= 60:
            return (
                "Input is highly abstract. Structural generation is paused until framing details are provided. "
                f"Missing elements: {', '.join(missing) if missing else 'none'}."
            )
        return f"Abstractness score is {score}. Current deepening level: {level.value}."
