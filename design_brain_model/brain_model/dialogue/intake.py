from ..memory.types import SemanticUnitL1
import time
import uuid

class InputIntake:
    """
    ユーザーの自由記述概要入力を受け取る (Spec Vol.2 Sec 3)
    """
    def intake(self, raw_input: str) -> SemanticUnitL1:
        return SemanticUnitL1(
            id=str(uuid.uuid4()),
            type="REQUIREMENT",
            content=raw_input,
            source="USER",
            timestamp=float(time.time())
        )
