from typing import List, Any, Dict
from ..memory.types import SemanticUnit
from .base import BaseCoreA

class ExplorationCore(BaseCoreA):
    """
    Core-A: Exploration.
    Currently a Mock implementation that generates candidate units from input.
    """
    
    def generate_hypotheses(self, input_data: Dict[str, Any]) -> List[SemanticUnit]:
        """
        Mock generation logic.
        Extracts simple keywords to create 'Concept' units.
        """
        text = input_data.get("content", "")
        message_id = input_data.get("message_id")
        
        candidates = []
        
        # Mock Heuristic 1: "Database" -> Concept
        if "database" in text.lower():
            candidates.append(SemanticUnit(
                content="Database",
                type="concept",
                source_message_id=message_id
            ))
            
        # Mock Heuristic 2: "User" -> Concept
        if "user" in text.lower():
             candidates.append(SemanticUnit(
                content="User",
                type="concept",
                source_message_id=message_id
            ))

        # Mock Heuristic 3: Any other text -> Unknown (for testing discard path)
        if not candidates and text:
             candidates.append(SemanticUnit(
                content=text,
                type="unknown",
                source_message_id=message_id
            ))
            
        return candidates
