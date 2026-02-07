from .types import ReadinessReport
from ..memory.types import SemanticUnitL2, Stability

class ReadinessInspector:
    """
    現在の入力が「設計に活かせる状態か」を判定する (Spec Vol.2 Sec 5)
    """
    def inspect(self, unit: SemanticUnitL2) -> ReadinessReport:
        satisfied = []
        missing = []
        blocking = []

        # STABLE 判定条件 (Sec 5.4)
        required_fields = ["objective", "scope_in", "scope_out", "success_criteria"]
        
        for field in required_fields:
            if field in unit.confirmed_fields:
                satisfied.append(field)
            else:
                missing.append(field)

        # Blocking Issues 定義 (Sec 5.5) - 例：不整合チェック
        if "scope_in" in satisfied and "scope_out" in satisfied:
            # 簡単な不整合チェックの例
            common = set(unit.scope_in) & set(unit.scope_out)
            if common:
                blocking.append(f"scope_out が scope_in と重複・否定しています: {common}")

        # Stability 判定
        if not missing and not blocking:
            stability = Stability.STABLE
        elif len(satisfied) > 0:
            stability = Stability.PARTIAL
        else:
            stability = Stability.UNSTABLE

        return ReadinessReport(
            stability=stability,
            satisfied_requirements=satisfied,
            missing_requirements=missing,
            blocking_issues=blocking
        )