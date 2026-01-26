from typing import List, Dict, Any, Optional
from pydantic import BaseModel
from .memory.space import MemorySpace
from .memory.gate import MemoryGate
from .core.a_cycle import ExplorationCore
from .core.b_cycle import ValidationCore

class DesignCommandType(str):
    EXTRACT_SEMANTICS = "extract_semantics"
    PROPOSE_STRUCTURE = "propose_structure"

class DesignCommand(BaseModel):
    type: str  # DesignCommandType
    payload: Dict[str, Any]

class DesignResult(BaseModel):
    success: bool
    data: Dict[str, Any]
    message: Optional[str] = None

class DesignBrainModel:
    """
    Stateless Design Intelligence Engine.
    Phase 9: Internal Refactoring for MemorySpace and Core Separation.
    """
    def __init__(self):
        # 1. Initialize Memory Infrastructure
        self.memory_space = MemorySpace()
        self.memory_gate = MemoryGate(self.memory_space)
        
        # 2. Initialize Cores
        self.core_a = ExplorationCore()
        self.core_b = ValidationCore(self.memory_gate)
    
    def handle_design_command(self, command: DesignCommand) -> DesignResult:
        if command.type == DesignCommandType.EXTRACT_SEMANTICS:
            return self._run_extraction_pipeline(command.payload)
        elif command.type == DesignCommandType.PROPOSE_STRUCTURE:
             return self._run_structure_pipeline() # Simplified for now
        else:
            return DesignResult(success=False, data={}, message=f"Unknown command type: {command.type}")

    def _run_extraction_pipeline(self, payload: Dict[str, Any]) -> DesignResult:
        # 1. Core-A Generates Hypotheses
        candidates = self.core_a.generate_hypotheses(payload)
        
        processed_units = []
        for unit in candidates:
            # 2. Core-B Evaluates and (Try) Store
            evaluated_unit = self.core_b.evaluate(unit)
            processed_units.append(evaluated_unit.model_dump())
            
        return DesignResult(success=True, data={"units": processed_units})

    def _run_structure_pipeline(self) -> DesignResult:
        # Placeholder for structure proposal logic
        return DesignResult(success=True, data={"components": ["App", "DB (Placeholder)"]})
