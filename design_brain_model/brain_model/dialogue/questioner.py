from typing import List
from .types import QuestionTemplate, QuestionType, QuestionPriority, ReadinessReport
from ..memory.types import SemanticUnitL2

class QuestionAssignment:
    """
    未確定要素を確定させるための質問を割り当てる (Spec Vol.2 Sec 6, 7)
    """
    TEMPLATES = [
        QuestionTemplate(id="QT-01", target_field="objective", type=QuestionType.FILL, prompt="主目的（objective）を教えてください。", priority=QuestionPriority.HIGH),
        QuestionTemplate(id="QT-02", target_field="scope_in", type=QuestionType.FILL, prompt="やること（scope_in）を具体的に教えてください。", priority=QuestionPriority.HIGH),
        QuestionTemplate(id="QT-03", target_field="scope_out", type=QuestionType.FILL, prompt="やらないこと（scope_out）を明確にしてください。", priority=QuestionPriority.MEDIUM),
        QuestionTemplate(id="QT-06", target_field="success_criteria", type=QuestionType.FILL, prompt="どのような状態になれば成功（success_criteria）と言えますか？", priority=QuestionPriority.MEDIUM),
    ]

    def assign_questions(self, report: ReadinessReport) -> List[QuestionTemplate]:
        if report.blocking_issues:
            # blocking_issues がある場合、新規質問は出さない (Sec 7.2)
            return []

        # missing_requirements に対応するテンプレートを取得
        questions = [t for t in self.TEMPLATES if t.target_field in report.missing_requirements]
        
        # priority 順にソート (HIGH -> MEDIUM -> LOW)
        priority_map = {"HIGH": 0, "MEDIUM": 1, "LOW": 2}
        questions.sort(key=lambda q: priority_map[q.priority.value])

        # 最大3問 (Sec 7.1)
        return questions[:3]