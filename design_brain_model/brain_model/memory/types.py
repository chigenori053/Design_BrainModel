from enum import Enum
from typing import Dict, Any, Optional, Set, List, Union
from pydantic import BaseModel, Field, ConfigDict
import uuid
from dataclasses import dataclass, field
import numpy as np

# Assuming interface.py is in the same directory
from .interface import OpticalMemoryInterface, MemoryHit


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

# --- Phase16 ---
class OriginContext(str, Enum):
    TEXT = "text"
    VISION = "vision"
    MULTIMODAL = "multimodal"

@dataclass
class RecallResult:
    """Represents the semantic interpretation of a memory recall."""
    recalled: bool
    best_hit_id: Optional[str]
    resonance: float

class SemanticRepresentation(BaseModel):
    """
    Phase16: The smallest unit that semantically represents an input,
    can be reused as a common object for memory, reasoning, and judgment.
    """
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))

    # Core data
    # Using Any for now to avoid strict numpy dependency issues in pydantic
    # In practice, this will be np.ndarray of np.complex128
    semantic_representation: Any # Holographic representation (Complex^1024)
    structure_signature: Dict[str, Any]   # AST / Vision spectral structure

    # Metadata
    origin_context: OriginContext
    confidence: float = 0.0
    entropy: float = 0.0

    model_config = ConfigDict(arbitrary_types_allowed=True)
    
    # --- Phase16 Memory Interface Methods ---

    def export_for_memory(self) -> tuple[str, np.ndarray]:
        """
        Exports the ID and the normalized vector representation for memory storage.
        Ensures the vector is normalized.
        """
        vector = np.asarray(self.semantic_representation)
        norm = np.linalg.norm(vector)
        if norm == 0:
            # Return a zero vector if the original is zero to avoid division errors
            normalized_vector = vector
        else:
            normalized_vector = vector / norm
        
        return (str(self.id), normalized_vector)

    def recall(
        self,
        memory: OpticalMemoryInterface,
        top_k: int = 1
    ) -> List[MemoryHit]:
        """
        Queries a given memory with its own representation to find similar items.
        """
        _, vector_to_query = self.export_for_memory()
        # Query with k+1 to have a fallback if the top hit is itself
        return memory.query(
            vector=vector_to_query,
            top_k=top_k + 1 
        )

    def interpret_recall(
        self,
        hits: List[MemoryHit],
        threshold: float
    ) -> RecallResult:
        """
        Interprets the raw hits from memory based on a semantic threshold.
        Filters out self-recall and guarantees to update confidence.
        """
        filtered_hits = [h for h in hits if h.key != self.id]

        if not filtered_hits:
            result = RecallResult(recalled=False, best_hit_id=None, resonance=0.0)
        else:
            best_hit = filtered_hits[0]
            if best_hit.resonance >= threshold:
                result = RecallResult(
                    recalled=True,
                    best_hit_id=best_hit.key,
                    resonance=best_hit.resonance
                )
            else:
                result = RecallResult(
                    recalled=False,
                    best_hit_id=None,
                    resonance=best_hit.resonance
                )
        
        self.update_confidence_from_recall(result)
        return result

    def update_confidence_from_recall(self, recall_result: RecallResult):
        """
        Updates confidence and entropy based on the recall result, as per Phase16 spec.
        """
        self.confidence = max(0.0, min(1.0, recall_result.resonance))
        self.entropy = 1.0 - self.confidence

# --- End of Phase16 ---


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

# --- Phase17-3 Gate Specification Types ---

@dataclass(slots=True)
class SemanticUnitL1:
    """
    Represents an "undecided semantic unit."
    Structurally prohibits judgment, evaluation, and code links.
    """
    id: str
    type: str  # Fixed set
    content: str
    source: str
    timestamp: float
    used_in_l2_ids: List[str] = field(default_factory=list)

@dataclass(frozen=True)
class SemanticUnitL2:
    """
    Represents a decision with history, ensuring immutability.
    This corresponds to an L2-Atom-GEN.
    """
    id: str
    decision_polarity: bool
    evaluation: Dict[str, float]
    scope: Dict[str, Any]
    source_cluster_id: str
    source_l1_ids: List[str]

    def __post_init__(self):
        if not self.source_l1_ids:
            raise ValueError("source_l1_ids cannot be empty for an L2 unit.")

@dataclass(slots=True)
class L1Cluster:
    """
    Represents a cluster of L1 units.
    """
    id: str
    l1_ids: List[str]
