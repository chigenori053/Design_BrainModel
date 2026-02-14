from __future__ import annotations

import re
from typing import List, Optional

from .types import DesignStructureKind, DesignStructureUnit, new_id


class DesignStructureGenerator:
    """
    Generates structural hypotheses without final decisions.
    """

    _sentence_split = re.compile(r"(?<=[.!?。！？])\s+|\n+")

    def generate(self, text: str, source_text_id: Optional[str] = None) -> List[DesignStructureUnit]:
        sentences = [s.strip() for s in self._sentence_split.split(text) if s.strip()]
        units: List[DesignStructureUnit] = []

        for sentence in sentences:
            kind = self._classify(sentence)
            units.append(
                DesignStructureUnit(
                    id=new_id("struct"),
                    hypothesis=sentence,
                    kind=kind,
                    source_text_id=source_text_id,
                    metadata={"length": len(sentence)},
                )
            )

        return units

    def _classify(self, sentence: str) -> DesignStructureKind:
        lower = sentence.lower()

        if self._contains_any(lower, ["layer", "layering", "tier", "stack", "pipeline"]):
            return DesignStructureKind.LAYERING

        if self._contains_any(lower, ["decompose", "decomposition", "function", "workflow", "stage", "step"]):
            return DesignStructureKind.FUNCTIONAL_DECOMPOSITION

        if self._contains_any(lower, ["component", "module", "service", "agent", "subsystem"]):
            return DesignStructureKind.COMPONENT_CANDIDATE

        return DesignStructureKind.OTHER

    @staticmethod
    def _contains_any(text: str, needles: List[str]) -> bool:
        return any(n in text for n in needles)
