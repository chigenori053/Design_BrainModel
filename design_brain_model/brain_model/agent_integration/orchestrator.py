from __future__ import annotations

from typing import List, Optional

from .types import (
    IntegrationState,
    IntegrationStateName,
    OrchestrationResult,
    SemanticUnit,
)
from .semantic_agent import SemanticUnitGenerator
from .structure_agent import DesignStructureGenerator
from .integration_layer import IntegrationLayer
from .abstractness import AbstractnessAnalyzer
from .evaluation_agent import DesignEvaluationAgent
from .suggestion_agent import DesignSuggestionAgent
from .web_search_agent import WebSearchAgent


class DesignModelOrchestrator:
    """
    Orchestrates integrated agents per v0.9 specification.
    Keeps outputs parallel and non-destructive.
    """

    def __init__(self):
        self._semantic_agent = SemanticUnitGenerator()
        self._structure_agent = DesignStructureGenerator()
        self._integration_layer = IntegrationLayer()
        self._abstractness_analyzer = AbstractnessAnalyzer()
        self._evaluation_agent = DesignEvaluationAgent()
        self._suggestion_agent = DesignSuggestionAgent()
        self._web_search_agent = WebSearchAgent()
        self._history: List[OrchestrationResult] = []
        self._state = IntegrationState()

    @property
    def state(self) -> IntegrationState:
        return self._state

    @property
    def history(self) -> List[OrchestrationResult]:
        return list(self._history)

    def process_input(
        self,
        content: str,
        source_text_id: Optional[str] = None,
        explicit_search_query: Optional[str] = None,
    ) -> OrchestrationResult:
        self._state = IntegrationState(state=IntegrationStateName.INPUT_RECEIVED)

        semantic_units = self._semantic_agent.generate(content, source_text_id=source_text_id)
        self._state.state = IntegrationStateName.ABSTRACT_ANALYSIS
        framing_feedback = self._abstractness_analyzer.analyze(content, semantic_units)
        self._state.abstractness_score = framing_feedback.abstract_score
        self._state.deepening_level = framing_feedback.deepening_level
        self._state.framing_required = framing_feedback.abstract_score >= 60

        if self._state.framing_required:
            self._state.state = IntegrationStateName.FRAMING_REQUIRED
            self._state.ready_for_design = False
            result = OrchestrationResult(
                state=self._state,
                semantic_units=semantic_units,
                framing_feedback_units=[framing_feedback],
            )
            self._history.append(result)
            return result

        self._state.state = IntegrationStateName.ANALYSIS_RUNNING
        structure_units = self._structure_agent.generate(content, source_text_id=source_text_id)

        integration_units = self._integration_layer.integrate(semantic_units, structure_units)
        self._state.state = IntegrationStateName.INTEGRATED

        evaluation_units, eval_context = self._evaluation_agent.evaluate(
            semantic_units,
            structure_units=structure_units,
            integration_units=integration_units,
        )
        self._state.state = IntegrationStateName.EVALUATED

        missing_required = eval_context.get("missing_required", [])
        conflicts = eval_context.get("conflicts", [])
        high_severity_present = eval_context.get("high_severity_present", False)

        self._state.missing_required = list(missing_required)
        self._state.conflicts = list(conflicts)
        self._state.high_severity_present = bool(high_severity_present)

        self._state.ready_for_design = self._is_ready_for_design(
            missing_required,
            conflicts,
            high_severity_present,
            semantic_units,
            self._state.abstractness_score,
        )

        suggestion_units = self._suggestion_agent.generate(evaluation_units)
        self._state.external_knowledge_need_score = self._compute_external_knowledge_score(
            semantic_units,
            evaluation_units,
            suggestion_units,
            high_severity_present,
        )
        knowledge_units = []

        if not self._state.framing_required:
            if explicit_search_query:
                knowledge_units.append(self._web_search_agent.search(explicit_search_query, requested_by="human"))
            elif self._state.external_knowledge_need_score >= 60:
                query = self._derive_query(semantic_units)
                knowledge_units.append(self._web_search_agent.search(query, requested_by="model"))

        if knowledge_units:
            suggestion_units = self._suggestion_agent.generate(evaluation_units, knowledge_units=knowledge_units)

        result = OrchestrationResult(
            state=self._state,
            semantic_units=semantic_units,
            structure_units=structure_units,
            integration_units=integration_units,
            evaluation_units=evaluation_units,
            suggestion_units=suggestion_units,
            knowledge_units=knowledge_units,
            framing_feedback_units=[framing_feedback],
        )
        self._history.append(result)
        return result

    @staticmethod
    def _is_ready_for_design(
        missing_required: List[str],
        conflicts: List[str],
        high_severity_present: bool,
        semantic_units: List[SemanticUnit],
        abstractness_score: int,
    ) -> bool:
        if missing_required:
            return False
        if conflicts:
            return False
        if high_severity_present:
            return False
        if abstractness_score >= 60:
            return False
        has_objective = any(u.kind.value == "OBJECTIVE" for u in semantic_units)
        has_scope = any(u.kind.value == "SCOPE" for u in semantic_units)
        has_constraint = any(u.kind.value == "CONSTRAINT" for u in semantic_units)
        if not has_objective:
            return False
        if not (has_scope or has_constraint):
            return False
        return True

    @staticmethod
    def _compute_external_knowledge_score(
        semantic_units: List[SemanticUnit],
        evaluation_units: List,
        suggestion_units: List,
        high_severity_present: bool,
    ) -> int:
        score = 0
        missing_required = len([e for e in evaluation_units if e.issue_type.value == "missing"])
        ambiguity_count = len([e for e in evaluation_units if e.issue_type.value == "ambiguity"])
        dependency_count = len([e for e in evaluation_units if e.issue_type.value == "dependency"])
        total_units = max(1, len(semantic_units))

        # Missing critical info (max 30)
        score += min(30, missing_required * 15)

        # Ambiguity density (max 20)
        ambiguity_density = ambiguity_count / total_units
        score += min(20, int(ambiguity_density * 40))

        # External dependency (max 25)
        score += min(25, dependency_count * 12)

        # Suggestion demand (max 15)
        score += min(15, len(suggestion_units) * 5)

        # Low confidence penalty (max 10)
        if high_severity_present or any(u.kind.value == "OBJECTIVE" for u in semantic_units) is False:
            score += 10

        return min(100, score)

    @staticmethod
    def _derive_query(units: List[SemanticUnit]) -> str:
        objective = next((u for u in units if u.kind.value == "OBJECTIVE"), None)
        scope = next((u for u in units if u.kind.value == "SCOPE"), None)
        parts = []
        if objective:
            parts.append(objective.content)
        if scope:
            parts.append(scope.content)
        if not parts:
            parts.append("design requirements clarification")
        return " ".join(parts)[:200]
