from typing import List, Dict, Any, Optional
import time
from .intake import InputIntake
from .decomposer import Decomposer
from .inspector import ReadinessInspector
from .questioner import QuestionAssignment
from .builder import DesignCandidateBuilder
from .types import (
    DialogueState, DialoguePhase, DecomposedElements, 
    DesignCandidate, HumanOverrideLog, HumanOverrideAction
)
from ..memory.types import SemanticUnitL2, Stability

class DialogueController:
    """
    Dialogue Core / PhaseC 全体の制御点 (Spec Vol.2 & Vol.3)
    """
    def __init__(self):
        self.intake = InputIntake()
        self.decomposer = Decomposer()
        self.inspector = ReadinessInspector()
        self.questioner = QuestionAssignment()
        self.builder = DesignCandidateBuilder()
        self._state: Optional[DialogueState] = None
        self._override_logs: List[HumanOverrideLog] = []
        self._intake_confirmed_fields: set[str] = set()

    def start_dialogue(self, raw_input: str) -> DialogueState:
        l1_unit = self.intake.intake(raw_input)
        elements = self.decomposer.decompose(raw_input)
        
        unit_data = {
            "source_l1_ids": [l1_unit.id],
            "objective": elements.objective or "",
            "scope_in": elements.scope_in or [],
            "scope_out": elements.scope_out or [],
            "constraints": elements.constraints or [],
            "assumptions": elements.assumptions or [],
            "success_criteria": elements.success_criteria or [],
            "risks": [elements.risks] if elements.risks else [],
        }
        
        confirmed = set()
        for field, value in unit_data.items():
            if value and field not in ["source_l1_ids"]:
                confirmed.add(field)
        self._intake_confirmed_fields = set(confirmed)
        
        unit = SemanticUnitL2(**unit_data, confirmed_fields=confirmed)
        return self._refresh_state(unit)

    def submit_answer(self, target_field: str, answer: Any) -> DialogueState:
        if not self._state:
            raise RuntimeError("Dialogue not started.")

        unit = self._state.semantic_unit
        new_confirmed = set(unit.confirmed_fields)
        new_confirmed.add(target_field)
        
        new_unit = unit.model_copy(update={target_field: answer, "confirmed_fields": new_confirmed})
        return self._refresh_state(new_unit)

    def submit_divergence(self, raw_input: str) -> DialogueState:
        """
        候補に満足しない場合の再設計入力 (Spec Vol.3 Sec 9)
        """
        if not self._state:
            raise RuntimeError("Dialogue not started.")
            
        # SemanticUnit を PARTIAL/UNSTABLE に戻し、open_questions を再生成する
        unit = self._state.semantic_unit
        # 自由記述入力を Decompose して反映するが、既存の確信済みフィールドは維持または上書き検討
        elements = self.decomposer.decompose(raw_input)
        
        update_data = {}
        if elements.objective: update_data["objective"] = elements.objective
        # ... 他の要素も同様に反映

        # stability をリセットするために、confirmed_fields から一部を削除するなどの処理
        # ここでは単純に新しい入力を反映し、再度 Inspect する
        new_unit = unit.model_copy(update=update_data)
        return self._refresh_state(new_unit)

    def human_override(self, action: HumanOverrideAction, candidate_id: Optional[str] = None, reason: Optional[str] = None) -> DialogueState:
        """
        唯一の確定操作点 (Spec Vol.3 Sec 4.4)
        """
        if not self._state:
            raise RuntimeError("Dialogue not started.")
            
        log = HumanOverrideLog(
            unit_id=self._state.semantic_unit.id,
            candidate_id=candidate_id,
            action=action,
            reason=reason
        )
        self._override_logs.append(log)
        
        # 確定後は READONLY フェーズへ移行
        self._state = self._state.model_copy(update={"phase": DialoguePhase.READONLY})
        return self._state

    def _refresh_state(self, unit: SemanticUnitL2) -> DialogueState:
        # 1. Readiness Inspection
        report = self.inspector.inspect(unit)
        
        # 2. Stability 更新 (Frozen モデルなので再生成)
        unit = unit.model_copy(update={"stability": report.stability})
        
        # 3. Phase 判定
        phase = DialoguePhase.CLARIFYING
        candidates = []
        questions = []
        
        if report.stability == Stability.STABLE:
            candidates = self.builder.build_candidates(unit)
            if "scope_in" in self._intake_confirmed_fields:
                phase = DialoguePhase.STABLE
            else:
                phase = DialoguePhase.CANDIDATES_READY
        else:
            questions = self.questioner.assign_questions(report)

        self._state = DialogueState(
            semantic_unit=unit,
            readiness=report,
            open_questions=questions,
            candidates=candidates,
            phase=phase
        )
        return self._state

    def get_state(self) -> DialogueState:
        if not self._state:
            raise RuntimeError("Dialogue not started.")
        return self._state
