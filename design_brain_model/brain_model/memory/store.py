from abc import ABC, abstractmethod
from typing import Dict, List, Optional
from .types import SemanticUnit, MemoryType

class BaseMemory(ABC):
    def __init__(self, memory_type: MemoryType):
        self.memory_type = memory_type
        self.storage: Dict[str, SemanticUnit] = {}

    @abstractmethod
    def store(self, unit: SemanticUnit) -> bool:
        pass

    def retrieve(self, unit_id: str) -> Optional[SemanticUnit]:
        return self.storage.get(unit_id)
    
    def list_all(self) -> List[SemanticUnit]:
        return list(self.storage.values())

class PHS(BaseMemory):
    """Persistent Holographic Store: Stores everything that is ACCEPTED or REVIEW."""
    def __init__(self):
        super().__init__(MemoryType.PHS)

    def store(self, unit: SemanticUnit) -> bool:
        unit.memory_type = self.memory_type
        self.storage[unit.id] = unit
        return True

class SHM(BaseMemory):
    """Static Holographic Memory: Normalized knowledge. Only populated via Promotion."""
    def __init__(self):
        super().__init__(MemoryType.SHM)

    def store(self, unit: SemanticUnit) -> bool:
        # STRICT RULE: Direct storage only allowed via promotion logic in practice.
        # Ideally this method is protected, but for MVP we wrap it.
        unit.memory_type = self.memory_type
        self.storage[unit.id] = unit
        return True

class CHM(BaseMemory):
    """Causal Holographic Memory: Stores causal reasoning reasoning/summaries."""
    def __init__(self):
        super().__init__(MemoryType.CHM)

    def store(self, unit: SemanticUnit) -> bool:
        unit.memory_type = self.memory_type
        self.storage[unit.id] = unit
        return True

class DHM(BaseMemory):
    """Dynamic Holographic Memory: Evolution mechanism. Currently Empty/Inactive."""
    def __init__(self):
        super().__init__(MemoryType.DHM)

    def store(self, unit: SemanticUnit) -> bool:
        # DHM is not active in Phase 9.
        return False
