from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field

from design_brain_model.brain_model.memory.types import SemanticUnit
from design_brain_model.hybrid_vm.control_layer.state import DecisionOutcome


class ExplanationLevel(str, Enum):
    BASIC = "BASIC"
    DETAIL = "DETAIL"


class Constraint(BaseModel):
    content: str
    scope: Optional[str] = None


class CausalSummary(BaseModel):
    summary: str
    sources: List[str] = Field(default_factory=list)


class LanguageInput(BaseModel):
    semantic_units: List[SemanticUnit] = Field(default_factory=list)
    decision: Optional[DecisionOutcome] = None
    constraints: List[Constraint] = Field(default_factory=list)
    causal_summary: Optional[CausalSummary] = None


class LanguageOutput(BaseModel):
    text: str
    explanation_level: ExplanationLevel


class LanguageEngine:
    """
    Phase10: Language Engine (stateless, deterministic).
    Converts meaning structures into language without mutating memory.
    """

    def generate(self, input_data: LanguageInput) -> LanguageOutput:
        linear_items = self._linearize(input_data)
        ordered_items = self._order_by_priority(linear_items)
        sentences = [self._map_to_sentence(item) for item in ordered_items]
        text = self._assemble(sentences)
        level = self._determine_level(input_data)
        return LanguageOutput(text=text, explanation_level=level)

    def _linearize(self, input_data: LanguageInput) -> List[dict]:
        items: List[dict] = []

        if input_data.decision is not None:
            items.append({"type": "decision", "value": input_data.decision})

        for constraint in input_data.constraints:
            items.append({"type": "constraint", "value": constraint})

        for unit in sorted(
            input_data.semantic_units,
            key=lambda u: (u.type, u.content, u.id),
        ):
            items.append({"type": "semantic_unit", "value": unit})

        if input_data.causal_summary is not None:
            items.append({"type": "causal_summary", "value": input_data.causal_summary})

        return items

    def _order_by_priority(self, items: List[dict]) -> List[dict]:
        priority = {
            "decision": 0,
            "constraint": 1,
            "semantic_unit": 2,
            "causal_summary": 3,
        }
        return sorted(items, key=lambda item: priority.get(item["type"], 99))

    def _map_to_sentence(self, item: dict) -> str:
        item_type = item["type"]
        value = item["value"]

        if item_type == "decision":
            status = value.consensus_status.value if value.consensus_status else "UNKNOWN"
            winner = value.ranked_candidates[0].content if value.ranked_candidates else "None"
            return f"Decision status: {status}. Selected candidate: {winner}."

        if item_type == "constraint":
            scope = f" Scope: {value.scope}." if value.scope else ""
            return f"Constraint: {value.content}.{scope}"

        if item_type == "semantic_unit":
            return f"Semantic unit ({value.type}): {value.content}."

        if item_type == "causal_summary":
            return (
                "This decision considered the following factors: "
                f"{value.summary}"
            )

        return "Unrecognized item."

    def _assemble(self, sentences: List[str]) -> str:
        if not sentences:
            return "No language output available."
        return "\n".join(sentences)

    def _determine_level(self, input_data: LanguageInput) -> ExplanationLevel:
        if input_data.causal_summary or input_data.constraints:
            return ExplanationLevel.DETAIL
        if len(input_data.semantic_units) > 3:
            return ExplanationLevel.DETAIL
        return ExplanationLevel.BASIC
