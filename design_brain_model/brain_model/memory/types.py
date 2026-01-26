from enum import Enum
from typing import Dict, Any, Optional, Set
from pydantic import BaseModel, Field
import uuid

class MemoryType(str, Enum):
    PHS = "PHS" # Persistent Holographic Store
    SHM = "SHM" # Static Holographic Memory
    CHM = "CHM" # Causal Holographic Memory
    DHM = "DHM" # Dynamic Holographic Memory

class Classification(str, Enum):
    GENERALIZABLE = "GENERALIZABLE"
    UNIQUE = "UNIQUE"
    DISCARDABLE = "DISCARDABLE"

class Decision(str, Enum):
    ACCEPT = "ACCEPT"
    REVIEW = "REVIEW"
    REJECT = "REJECT"

class SemanticUnit(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    content: str
    type: str  # concept, constraint, etc.
    source_message_id: Optional[str] = None
    
    # Metadata for Memory
    classification: Optional[Classification] = None
    decision: Optional[Decision] = None
    memory_type: Optional[MemoryType] = None
    
    # Relationships
    related_unit_ids: Set[str] = Field(default_factory=set)

    class Config:
        frozen = False # Allow updates within safe boundaries
