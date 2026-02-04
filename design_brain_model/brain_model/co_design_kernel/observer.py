from typing import Any
from .types import InputData

class InputObserver:
    """
    Responsible for accepting raw input and converting it into a formal InputData observation.
    Does not interpret intent (command vs question).
    """
    def observe(self, content: Any, source: str = "user") -> InputData:
        # In a real system, this might do some pre-processing or normalization.
        # For Phase20-A, we treat everything as raw content.
        
        content_str = str(content)
        return InputData(
            content=content_str,
            source=source
        )
