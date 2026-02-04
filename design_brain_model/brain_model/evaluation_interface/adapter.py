from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any
from enum import Enum

from design_brain_model.brain_model.memory.types import Decision, StoreType, MemoryStatus
from design_brain_model.hybrid_vm.control_layer.state import DecisionOutcome, SemanticUnit, DecisionNode

# --- DTO Definitions (Immutable / Read-Only) ---

@dataclass(frozen=True)
class DecisionDTO:
    label: str
    confidence: float
    entropy: float
    utility: float
    reason: str

@dataclass(frozen=True)
class MemoryDTO:
    store: str
    status: str
    semantic_id: Optional[str] = None

@dataclass(frozen=True)
class DesignEvalDTO:
    completeness: float
    consistency: float
    warnings: List[str] = field(default_factory=list)

@dataclass(frozen=True)
class InteractionResultDTO:
    input: str
    response_text: str
    decision: Optional[DecisionDTO]
    memory: Optional[MemoryDTO]
    design_eval: Optional[DesignEvalDTO]

# --- Adapter Implementation ---

class AdapterLayer:
    """
    Phase 19-3: Adapter Layer
    Translates internal domain models into Read-Only DTOs for the UI.
    """

    @staticmethod
    def to_decision_dto(outcome: Optional[DecisionOutcome]) -> Optional[DecisionDTO]:
        if not outcome:
            return None
        
        # Calculate aggregate metrics if multiple evaluations exist
        # For MVP, taking the first or average. Here we take simplified view.
        confidence = 0.0
        entropy = 0.0
        utility = 0.0
        
        if outcome.evaluations:
            # Simple average for visualization
            confidence = sum(e.confidence for e in outcome.evaluations) / len(outcome.evaluations)
            entropy = sum(e.entropy for e in outcome.evaluations) / len(outcome.evaluations)
            
            # Sum utility vector components
            total_u = 0.0
            for e in outcome.evaluations:
                uv = e.utility_vector
                total_u += (uv.performance + uv.cost + uv.maintainability + uv.scalability + uv.risk) / 5.0
            
            utility = total_u / len(outcome.evaluations)

        return DecisionDTO(
            label=outcome.consensus_status.value if outcome.consensus_status else "UNKNOWN",
            confidence=confidence,
            entropy=entropy,
            utility=utility,
            reason=outcome.human_reason or outcome.explanation or ""
        )

    @staticmethod
    def to_memory_dto(unit: Optional[SemanticUnit]) -> Optional[MemoryDTO]:
        if not unit:
            return None
        
        # Mapping SemanticUnit status to StoreType/MemoryStatus concept for UI
        # This is a simplification logic:
        # UNSTABLE/REVIEW -> Working/Quarantine logic (simplified)
        # STABLE -> Canonical
        
        store = "UNKNOWN"
        status = unit.status.value if unit.status else "UNKNOWN"
        status_upper = status.upper()
        
        # Heuristic mapping for Phase 19-3
        if "UNSTABLE" in status_upper:
             store = StoreType.WORKING.value
        elif "STABLE" in status_upper:
            store = StoreType.CANONICAL.value
        elif "REVIEW" in status_upper:
            store = StoreType.QUARANTINE.value
            
        return MemoryDTO(
            store=store,
            status=status,
            semantic_id=unit.id
        )

    @staticmethod
    def to_design_eval_dto(eval_result: Dict[str, Any]) -> DesignEvalDTO:
        return DesignEvalDTO(
            completeness=eval_result.get("completeness_score", 0.0),
            consistency=eval_result.get("consistency_score", 0.0),
            warnings=eval_result.get("ambiguity_flags", []) + eval_result.get("missing_slots", [])
        )