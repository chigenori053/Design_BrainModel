from __future__ import annotations

from typing import Dict, Any

from .math_core import AXIS_NAMES
from ..knowledge import KnowledgeUnit


class DesignAgent:
    """
    PhaseA: DesignAgent（最小実装）
    KnowledgeUnit を参照し、設計案（構造）を生成する。
    """
    def generate_design_from_knowledge(self, knowledge_unit: KnowledgeUnit) -> Dict[str, Any]:
        # KnowledgeUnit を直接評価対象にしないため、設計案へ変換
        return {
            "design_from_knowledge_id": knowledge_unit.id,
            "abstract_structure": knowledge_unit.abstract_structure,
            "constraints": [c.model_dump() for c in knowledge_unit.constraints],
            "applicability_scope": knowledge_unit.applicability_scope.model_dump(),
            "axis_hints": AXIS_NAMES,
        }

    def generate_design_from_evidence(self, evidence_record: Dict[str, Any]) -> Dict[str, Any]:
        # Evidence は評価対象ではないため、設計案へ変換
        return {
            "design_from_evidence_id": evidence_record.get("id"),
            "evidence_summary": evidence_record.get("summary"),
            "constraints": evidence_record.get("constraints", []),
        }

