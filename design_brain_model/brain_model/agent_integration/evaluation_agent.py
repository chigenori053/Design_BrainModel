from __future__ import annotations

import re
from typing import Any, Dict, List, Set, Tuple

from .types import (
    DesignStructureUnit,
    EvaluationSemanticUnit,
    IntegrationAlignment,
    IntegrationMappingUnit,
    IssueType,
    SemanticUnit,
    SemanticUnitKind,
    Severity,
    new_id,
)


class DesignEvaluationAgent:
    """
    Evaluates SemanticUnits for missing items, conflicts, dependencies, and ambiguities.
    Does not modify SemanticUnits.
    """

    _ambiguous_terms = {
        "maybe",
        "some",
        "various",
        "etc",
        "approximately",
        "roughly",
        "tbd",
        "to be decided",
    }
    _dependency_terms = {
        "depends on",
        "dependent on",
        "requires",
        "integrate with",
        "third-party",
        "external",
        "api",
        "vendor",
        "regulation",
        "law",
        "compliance",
    }
    _negation_terms = {"not", "no", "never", "without", "exclude", "out", "out-of-scope"}

    def evaluate(
        self,
        units: List[SemanticUnit],
        structure_units: List[DesignStructureUnit] | None = None,
        integration_units: List[IntegrationMappingUnit] | None = None,
    ) -> Tuple[List[EvaluationSemanticUnit], Dict[str, Any]]:
        evaluations: List[EvaluationSemanticUnit] = []
        context: Dict[str, Any] = {}

        missing_required = self._detect_missing_required(units)
        for missing in missing_required:
            evaluations.append(
                EvaluationSemanticUnit(
                    id=new_id("eval"),
                    issue_type=IssueType.MISSING,
                    severity=Severity.HIGH,
                    description=f"Missing required semantic type: {missing}.",
                )
            )

        scope_conflicts = self._detect_scope_conflicts(units)
        for conflict in scope_conflicts:
            evaluations.append(
                EvaluationSemanticUnit(
                    id=new_id("eval"),
                    issue_type=IssueType.CONFLICT,
                    severity=Severity.HIGH,
                    description=conflict,
                )
            )

        negation_conflicts = self._detect_negation_conflicts(units)
        for conflict in negation_conflicts:
            evaluations.append(
                EvaluationSemanticUnit(
                    id=new_id("eval"),
                    issue_type=IssueType.CONFLICT,
                    severity=Severity.HIGH,
                    description=conflict,
                )
            )

        for unit in units:
            if self._is_ambiguous(unit.content):
                evaluations.append(
                    EvaluationSemanticUnit(
                        id=new_id("eval"),
                        issue_type=IssueType.AMBIGUITY,
                        severity=Severity.LOW,
                        description=f"Ambiguity detected in: '{unit.content}'.",
                        source_unit_ids=[unit.id],
                    )
                )

            if self._has_dependency(unit.content):
                evaluations.append(
                    EvaluationSemanticUnit(
                        id=new_id("eval"),
                        issue_type=IssueType.DEPENDENCY,
                        severity=Severity.MEDIUM,
                        description=f"External dependency implied in: '{unit.content}'.",
                        source_unit_ids=[unit.id],
                    )
                )

        integration_conflicts = self._detect_integration_conflicts(integration_units or [])
        for conflict in integration_conflicts:
            evaluations.append(
                EvaluationSemanticUnit(
                    id=new_id("eval"),
                    issue_type=IssueType.CONFLICT,
                    severity=Severity.MEDIUM,
                    description=conflict,
                )
            )

        context["missing_required"] = missing_required
        context["conflicts"] = scope_conflicts + negation_conflicts + integration_conflicts
        context["high_severity_present"] = any(e.severity == Severity.HIGH for e in evaluations)
        return evaluations, context

    def _detect_missing_required(self, units: List[SemanticUnit]) -> List[str]:
        required = {SemanticUnitKind.OBJECTIVE, SemanticUnitKind.SCOPE, SemanticUnitKind.CONSTRAINT}
        present = {u.kind for u in units}
        missing = [kind.value for kind in required if kind not in present]
        return missing

    def _detect_scope_conflicts(self, units: List[SemanticUnit]) -> List[str]:
        in_scope: Set[str] = set()
        out_scope: Set[str] = set()

        for unit in units:
            if unit.kind != SemanticUnitKind.SCOPE:
                continue
            in_items, out_items = self._extract_scope_items(unit.content)
            in_scope.update(in_items)
            out_scope.update(out_items)

        conflicts = []
        overlap = in_scope.intersection(out_scope)
        for item in sorted(overlap):
            conflicts.append(f"Scope conflict detected for item: '{item}'.")
        return conflicts

    def _extract_scope_items(self, content: str) -> Tuple[Set[str], Set[str]]:
        lower = content.lower()
        in_scope: Set[str] = set()
        out_scope: Set[str] = set()

        def split_items(segment: str) -> List[str]:
            return [s.strip() for s in re.split(r"[,;/]", segment) if s.strip()]

        if "in scope" in lower:
            part = lower.split("in scope", 1)[-1]
            in_scope.update(split_items(part))

        if "out of scope" in lower or "out-of-scope" in lower:
            if "out of scope" in lower:
                part = lower.split("out of scope", 1)[-1]
            else:
                part = lower.split("out-of-scope", 1)[-1]
            out_scope.update(split_items(part))

        return in_scope, out_scope

    def _detect_negation_conflicts(self, units: List[SemanticUnit]) -> List[str]:
        conflicts: List[str] = []
        normalized_units: List[Tuple[SemanticUnit, Set[str], bool]] = []

        for unit in units:
            tokens = self._tokenize(unit.content)
            negated = any(term in tokens for term in self._negation_terms)
            normalized_units.append((unit, tokens, negated))

        for i, (unit_a, tokens_a, neg_a) in enumerate(normalized_units):
            for unit_b, tokens_b, neg_b in normalized_units[i + 1 :]:
                if neg_a == neg_b:
                    continue
                overlap = tokens_a.intersection(tokens_b)
                if overlap:
                    conflicts.append(
                        f"Potential negation conflict between '{unit_a.content}' and '{unit_b.content}'."
                    )

        return conflicts

    def _detect_integration_conflicts(self, mappings: List[IntegrationMappingUnit]) -> List[str]:
        conflicts: List[str] = []
        for mapping in mappings:
            if mapping.alignment == IntegrationAlignment.MISMATCH:
                conflicts.append(mapping.description)
            elif mapping.alignment == IntegrationAlignment.UNMAPPED:
                if mapping.semantic_unit_ids:
                    conflicts.append("Semantic content not mapped to any structural hypothesis.")
                elif mapping.structure_unit_ids:
                    conflicts.append("Structural hypothesis not grounded in semantic content.")
        return conflicts

    def _is_ambiguous(self, content: str) -> bool:
        lower = content.lower()
        if len(content.split()) <= 4:
            return True
        return any(term in lower for term in self._ambiguous_terms)

    def _has_dependency(self, content: str) -> bool:
        lower = content.lower()
        return any(term in lower for term in self._dependency_terms)

    @staticmethod
    def _tokenize(text: str) -> Set[str]:
        tokens = re.findall(r"[a-z0-9\-]+", text.lower())
        stopwords = {"the", "a", "an", "and", "or", "of", "to", "in", "on", "for", "with"}
        return {t for t in tokens if t not in stopwords}
