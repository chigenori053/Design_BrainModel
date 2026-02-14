from __future__ import annotations

import re
from typing import List, Set

from .types import (
    DesignStructureUnit,
    IntegrationAlignment,
    IntegrationMappingUnit,
    SemanticUnit,
    new_id,
)


class IntegrationLayer:
    """
    Maps meaning to structure and detects alignment or mismatch.
    """

    def integrate(
        self,
        semantic_units: List[SemanticUnit],
        structure_units: List[DesignStructureUnit],
    ) -> List[IntegrationMappingUnit]:
        mappings: List[IntegrationMappingUnit] = []
        mapped_structure_ids: Set[str] = set()

        for semantic in semantic_units:
            semantic_tokens = self._tokenize(semantic.content)
            best_match = None
            best_overlap = 0

            for structure in structure_units:
                overlap = len(semantic_tokens.intersection(self._tokenize(structure.hypothesis)))
                if overlap > best_overlap:
                    best_overlap = overlap
                    best_match = structure

            if best_match and best_overlap >= 2:
                mapped_structure_ids.add(best_match.id)
                mappings.append(
                    IntegrationMappingUnit(
                        id=new_id("map"),
                        alignment=IntegrationAlignment.ALIGNED,
                        description="Semantic content aligns with structural hypothesis.",
                        semantic_unit_ids=[semantic.id],
                        structure_unit_ids=[best_match.id],
                        evidence={"overlap": best_overlap},
                    )
                )
            else:
                mappings.append(
                    IntegrationMappingUnit(
                        id=new_id("map"),
                        alignment=IntegrationAlignment.UNMAPPED,
                        description="Semantic content not mapped to any structural hypothesis.",
                        semantic_unit_ids=[semantic.id],
                        structure_unit_ids=[],
                    )
                )

        for structure in structure_units:
            if structure.id in mapped_structure_ids:
                continue
            mappings.append(
                IntegrationMappingUnit(
                    id=new_id("map"),
                    alignment=IntegrationAlignment.UNMAPPED,
                    description="Structural hypothesis not grounded in semantic content.",
                    semantic_unit_ids=[],
                    structure_unit_ids=[structure.id],
                )
            )

        return mappings

    @staticmethod
    def _tokenize(text: str) -> Set[str]:
        tokens = re.findall(r"[a-z0-9\\-]+", text.lower())
        stopwords = {"the", "a", "an", "and", "or", "of", "to", "in", "on", "for", "with"}
        return {t for t in tokens if t not in stopwords}
