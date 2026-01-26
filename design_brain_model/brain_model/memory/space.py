from typing import Optional, List
from .store import PHS, SHM, CHM, DHM
from .types import SemanticUnit, MemoryType, Classification

class MemorySpace:
    """
    The container for all Memory Types.
    """
    def __init__(self):
        self.phs = PHS()
        self.shm = SHM()
        self.chm = CHM()
        self.dhm = DHM()

    def get_memory(self, m_type: MemoryType):
        if m_type == MemoryType.PHS:
            return self.phs
        elif m_type == MemoryType.SHM:
            return self.shm
        elif m_type == MemoryType.CHM:
            return self.chm
        elif m_type == MemoryType.DHM:
            return self.dhm
        return None

    def promote_to_shm(self, unit: SemanticUnit) -> bool:
        """
        Explicit Promotion Trigger.
        In Phase 9, this is manual or triggered by specific rules, not automatic.
        """
        if unit.classification != Classification.GENERALIZABLE:
            return False
            
        # Clone unit for SHM (or move it, depending on policy. Here we clone as it exists in PHS history)
        # Deep copy simulation
        shm_unit = unit.model_copy()
        return self.shm.store(shm_unit)
