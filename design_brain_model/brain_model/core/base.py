from abc import ABC, abstractmethod
from typing import List, Any
from ..memory.types import SemanticUnit

class BaseCore(ABC):
    """Abstract Base Class for Brain Cores"""
    pass

class BaseCoreA(BaseCore):
    """
    Core-A: Exploration & Generation.
    Responsible for generating hypotheses, structures, and SemanticUnits.
    """
    @abstractmethod
    def generate_hypotheses(self, input_data: Any) -> List[SemanticUnit]:
        pass

class BaseCoreB(BaseCore):
    """
    Core-B: Validation & Decision.
    Responsible for evaluating units and making Decisions (ACCEPT/REVIEW/REJECT).
    """
    @abstractmethod
    def evaluate(self, unit: SemanticUnit) -> SemanticUnit:
        pass
