from enum import Enum
from typing import Dict, Any, Optional, Set, List, Union
from pydantic import BaseModel, Field, ConfigDict, model_validator
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

class StoreType(str, Enum):
    CANONICAL = "CanonicalStore"
    QUARANTINE = "QuarantineStore"
    WORKING = "WorkingMemory"

class MemoryStatus(str, Enum):
    ACTIVE = "ACTIVE"
    FROZEN = "FROZEN"
    DISABLED = "DISABLED"

class Classification(str, Enum):
    GENERALIZABLE = "GENERALIZABLE"
    UNIQUE = "UNIQUE"
    DISCARDABLE = "DISCARDABLE"

class Decision(str, Enum):
    ACCEPT = "ACCEPT"
    REVIEW = "REVIEW"
    REJECT = "REJECT"

DecisionLabel = Decision

class DecisionResult(BaseModel):
    label: Decision
    confidence: float
    entropy: float
    utility: float
    reason: str

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
    decision_label: Optional[DecisionLabel] = None
    memory_type: Optional[MemoryType] = None
    
    # Spec-02 Status fields
    status: MemoryStatus = MemoryStatus.ACTIVE
    status_changed_at: float = Field(default_factory=lambda: 0.0)
    status_reason: Optional[str] = None

    # Spec-03 Initial Metadata (Immutable history)
    confidence_init: float = 0.0
    decision_reason: Optional[str] = None

    # Spec-04 Evaluation Metrics
    reuse_count: int = 0
    accept_support_count: int = 0
    reject_impact_count: int = 0
    avg_EU_delta: float = 0.0
    retention_score: float = 0.0
    
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

class Stability(str, Enum):
    UNSTABLE = "UNSTABLE"
    PARTIAL = "PARTIAL"
    STABLE = "STABLE"

class SemanticUnitL2(BaseModel):
    """
    Represents a minimal and stable design representation as per Dialogue Spec Vol.1.
    Includes both the structural design elements and the decision history.
    """
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    
    # --- Dialogue Spec Vol.1 Minimal Structure ---
    objective: str = ""
    scope_in: List[str] = Field(default_factory=list)
    scope_out: List[str] = Field(default_factory=list)
    constraints: List[str] = Field(default_factory=list)
    assumptions: List[str] = Field(default_factory=list)
    success_criteria: List[str] = Field(default_factory=list)
    risks: List[str] = Field(default_factory=list)
    divergence_policy: str = ""
    open_questions: List[str] = Field(default_factory=list)
    stability: Stability = Stability.UNSTABLE
    confirmed_fields: Set[str] = Field(default_factory=set) # New in Dialogue Spec Vol.2

    # --- Evolution / Decision Metadata (Compatible with Phase 17-3) ---
    decision_polarity: bool = True
    evaluation: Dict[str, float] = Field(default_factory=dict)
    source_cluster_id: Optional[str] = None
    source_l1_ids: List[str] = Field(default_factory=list)

    model_config = ConfigDict(frozen=True)

    @model_validator(mode="after")
    def _validate_l2(self) -> "SemanticUnitL2":
        if not self.source_l1_ids:
            raise ValueError("source_l1_ids cannot be empty for an L2 unit.")
        return self

@dataclass(slots=True)
class L1Cluster:
    """
    Represents a cluster of L1 units.
    """
    id: str
    l1_ids: List[str]
