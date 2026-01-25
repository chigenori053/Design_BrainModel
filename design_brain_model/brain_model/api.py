from typing import List, Dict, Any, Optional
from pydantic import BaseModel

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
    Phase 0: Mock implementation using heuristics.
    """
    
    def handle_design_command(self, command: DesignCommand) -> DesignResult:
        if command.type == DesignCommandType.EXTRACT_SEMANTICS:
            return self._extract_semantics(command.payload)
        elif command.type == DesignCommandType.PROPOSE_STRUCTURE:
            return self._propose_structure(command.payload)
        else:
            return DesignResult(success=False, data={}, message=f"Unknown command type: {command.type}")

    def _extract_semantics(self, payload: Dict[str, Any]) -> DesignResult:
        """
        Mock: Extract semantic units from user text.
        Simple heuristic: Lines starting with '*' are Constraints.
        """
        text = payload.get("content", "")
        message_id = payload.get("message_id")
        
        extracted_units = []
        
        # Simple Mock Logic
        if "database" in text.lower():
            extracted_units.append({
                "id": f"unit_{len(text)}",
                "type": "concept",
                "content": "Database",
                "source_message_id": message_id
            })
            
        if "must" in text.lower():
             extracted_units.append({
                "id": f"unit_cons_{len(text)}",
                "type": "constraint",
                "content": text, # In reality, we'd extract the constraint phrase
                "source_message_id": message_id
            })

        return DesignResult(success=True, data={"units": extracted_units})

    def _propose_structure(self, payload: Dict[str, Any]) -> DesignResult:
        return DesignResult(success=True, data={"components": ["App", "DB"]})
