from __future__ import annotations

from typing import List

from .types import DesignKnowledgeUnit, DesignSuggestionUnit, EvaluationSemanticUnit, IssueType, new_id


class DesignSuggestionAgent:
    """
    Generates non-directive prompts to stimulate user thinking.
    """

    def generate(
        self,
        evaluations: List[EvaluationSemanticUnit],
        knowledge_units: List[DesignKnowledgeUnit] | None = None,
    ) -> List[DesignSuggestionUnit]:
        suggestions: List[DesignSuggestionUnit] = []
        _ = knowledge_units

        for evaluation in evaluations:
            prompt, tags = self._prompt_for_issue(evaluation)
            if not prompt:
                continue
            suggestions.append(
                DesignSuggestionUnit(
                    id=new_id("suggest"),
                    prompt=prompt,
                    related_issue_ids=[evaluation.id],
                    tags=tags,
                    options=self._default_options(evaluation.issue_type),
                )
            )

        return suggestions

    def _prompt_for_issue(self, evaluation: EvaluationSemanticUnit) -> tuple[str, List[str]]:
        if evaluation.issue_type == IssueType.MISSING:
            if "OBJECTIVE" in evaluation.description:
                return ("What is the primary objective you want to achieve?", ["objective"]) 
            if "SCOPE" in evaluation.description:
                return ("What should be explicitly in scope and out of scope?", ["scope"])
            if "CONSTRAINT" in evaluation.description:
                return ("Are there constraints or non-negotiables we should record?", ["constraint"])
            return ("Is there a required element that should be captured?", ["missing"])

        if evaluation.issue_type == IssueType.CONFLICT:
            return ("There may be a conflict in the current statements. Which part should be prioritized or clarified?", ["conflict"])

        if evaluation.issue_type == IssueType.DEPENDENCY:
            return ("Which external dependency is implied, and what assumptions do we have about it?", ["dependency"])

        if evaluation.issue_type == IssueType.AMBIGUITY:
            return ("Could you clarify the ambiguous part so it can be interpreted consistently?", ["ambiguity"])

        return ("", [])

    def _default_options(self, issue_type: IssueType) -> List[str]:
        if issue_type == IssueType.MISSING:
            return ["Add a statement", "Defer for later"]
        if issue_type == IssueType.CONFLICT:
            return ["Clarify priority", "Split into separate cases"]
        if issue_type == IssueType.DEPENDENCY:
            return ["Name the dependency", "Note uncertainty"]
        if issue_type == IssueType.AMBIGUITY:
            return ["Provide concrete example", "Define terms"]
        return []
