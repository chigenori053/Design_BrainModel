from __future__ import annotations

import re
from typing import List, Optional

from .types import SemanticUnit, SemanticUnitKind, new_id


class SemanticUnitGenerator:
    """
    SemanticUnit generation agent.
    Splits input into minimal units and classifies them without evaluation.
    """

    _sentence_split = re.compile(r"(?<=[.!?。！？])\s+|\n+")

    def generate(self, text: str, source_text_id: Optional[str] = None) -> List[SemanticUnit]:
        sentences = [s.strip() for s in self._sentence_split.split(text) if s.strip()]
        units: List[SemanticUnit] = []

        for sentence in sentences:
            kind = self._classify(sentence)
            units.append(
                SemanticUnit(
                    id=new_id("sem"),
                    content=sentence,
                    kind=kind,
                    source_text_id=source_text_id,
                    metadata={"length": len(sentence)}
                )
            )

        return units

    def _classify(self, sentence: str) -> SemanticUnitKind:
        lower = sentence.lower()

        if self._contains_any(lower, ["goal", "objective", "aim", "purpose", "we want", "we need to", "we are trying to"]):
            return SemanticUnitKind.OBJECTIVE

        if self._contains_any(lower, ["scope", "in scope", "out of scope", "out-of-scope"]):
            return SemanticUnitKind.SCOPE

        if self._contains_any(lower, ["must", "should", "require", "constraint", "cannot", "can't", "must not", "prohibit"]):
            return SemanticUnitKind.CONSTRAINT

        if self._contains_any(lower, ["assume", "assuming", "assumption", "probably", "likely"]):
            return SemanticUnitKind.ASSUMPTION

        if self._looks_like_fact(lower):
            return SemanticUnitKind.FACT

        return SemanticUnitKind.OTHER

    @staticmethod
    def _contains_any(text: str, needles: List[str]) -> bool:
        return any(n in text for n in needles)

    @staticmethod
    def _looks_like_fact(text: str) -> bool:
        if re.search(r"\b\d+\b", text):
            return True
        if any(token in text for token in [" is ", " are ", " was ", " were "]):
            return True
        return False
