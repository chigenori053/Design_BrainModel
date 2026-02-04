from typing import Dict, Any, Optional, List
from datetime import datetime

from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.events import UserInputEvent, EventType, Actor
from design_brain_model.hybrid_vm.control_layer.state import DecisionCandidate, Policy, Role, DecisionOutcome, Evaluation, UtilityVector
from design_brain_model.brain_model.memory.types import DecisionResult, Decision, StoreType, MemoryStatus, SemanticRepresentation, OriginContext
from design_brain_model.brain_model.memory.space import MemorySpace

from design_brain_model.brain_model.evaluation_interface.adapter import AdapterLayer, InteractionResultDTO
from design_brain_model.brain_model.evaluation_interface.design_eval_dhm import DesignEvalDHM
from design_brain_model.brain_model.evaluation_interface.language_dhm import LanguageDHM
from design_brain_model.brain_model.evaluation_interface.state_evaluation_engine import StateEvaluationEngine

class UiApi:
    """
    Phase 19-3: UI API (Read-Only Observation Layer)
    The single entry point for CLI and Desktop Apps.
    Orchestrates the interaction with HybridVM and Adapters.
    """

    def __init__(self):
        self.memory_space = MemorySpace()
        self.memory_space.canonical.load()
        self.memory_space.quarantine.load()
        
        self.vm = HybridVM()
        # VM also needs to be synced if we want full state persistence, 
        # but for Phase 19-3a, Memory persistence is key.
        self.design_eval = DesignEvalDHM()
        self.language_engine = LanguageDHM()
        self.state_eval_engine = StateEvaluationEngine()
        
        # Local log buffer for StateEvaluationEngine (simulating persistence)
        self._execution_logs = {
            "decisions": [],
            "memory": [],
            "recall": [],
            "design_eval": []
        }

    def run_interaction(self, input_text: str, context: Optional[Dict] = None) -> InteractionResultDTO:
        """
        Executes a full interaction cycle:
        Input -> VM -> Decision (Mock/Forced) -> Eval -> Language -> DTO
        """
        # 1. Send Input to HybridVM
        event_payload = {"content": input_text}
        self.vm.process_event(UserInputEvent(
            type=EventType.USER_INPUT,
            payload=event_payload,
            actor=Actor.USER
        ))

        # 2. Identify the Semantic Unit (Mocking the Brain's attention)
        # In a real loop, we'd pick the unit created by the input.
        # HybridVM mock creates a unit for every input msg.
        # We find the latest unit.
        latest_unit_id = None
        latest_unit = None
        if self.vm._state.semantic_units.units:
            latest_unit_id = list(self.vm._state.semantic_units.units.keys())[-1]
            latest_unit = self.vm._state.semantic_units.units[latest_unit_id]

        decision_outcome = None
        eval_result = {}
        response_text = "No decision made."

        if latest_unit:
            # 3. Force a Decision Evaluation (Mocking the Proposer)
            # We create a candidate that represents "Accepting this unit"
            candidate = DecisionCandidate(
                candidate_id=f"cand-{latest_unit.id}",
                resolves_question_id=f"q-{latest_unit.id}",
                proposed_by=Role.BRAIN,
                content=f"Accept unit {latest_unit.content}"
            )
            
            # Policy (Default)
            policy = Policy(name="default-policy", weights={"performance": 1.0})

            # Execute Decision via VM
            self.vm.evaluate_decision(
                question_id=f"q-{latest_unit.id}",
                candidates=[candidate],
                policy=policy
            )
            
            # Retrieve the Outcome
            if self.vm._state.decision_state.outcomes:
                decision_outcome = self.vm._state.decision_state.outcomes[-1]

        # 4. Design Eval & Language Generation
        if decision_outcome:
            # Convert internal Outcome to Phase19 Types for Eval
            # Map VM Outcome -> DecisionResult
            # Note: DecisionResult is from memory.types, DecisionOutcome is from vm.control_layer.state
            # We need to map them.
            
            # Logic to map ConsensusStatus to Decision Enum
            status_map = {
                "ACCEPT": Decision.ACCEPT,
                "REJECT": Decision.REJECT,
                "REVIEW": Decision.REVIEW,
                "ESCALATE": Decision.REVIEW # Fallback
            }
            d_label = status_map.get(decision_outcome.consensus_status.value, Decision.REVIEW)
            
            # Get avg metrics
            avg_conf = 0.0
            avg_ent = 0.0
            avg_util = 0.0
            if decision_outcome.evaluations:
                 avg_conf = sum(e.confidence for e in decision_outcome.evaluations) / len(decision_outcome.evaluations)
                 avg_ent = sum(e.entropy for e in decision_outcome.evaluations) / len(decision_outcome.evaluations)
                 # Calculate utility from vector (simple avg of fields)
                 for e in decision_outcome.evaluations:
                     uv = e.utility_vector
                     # Simple average of the 5 standard dimensions
                     score = (uv.performance + uv.cost + uv.maintainability + uv.scalability + uv.risk) / 5.0
                     avg_util += score
                 avg_util /= len(decision_outcome.evaluations)

            decision_res = DecisionResult(
                label=d_label,
                confidence=avg_conf,
                entropy=avg_ent,
                utility=avg_util,
                reason=decision_outcome.explanation or ""
            )

            # Map Memory State
            # Adapter logic reused or re-implemented for internal consumption?
            # We need dict format for DesignEvalDHM
            mem_dto = AdapterLayer.to_memory_dto(latest_unit)
            memory_state = {
                "store_type": mem_dto.store,
                "status": mem_dto.status
            }

            # Map Semantic Unit
            # Create a simplified SemanticRepresentation for Eval
            sem_rep = SemanticRepresentation(
                semantic_representation=[0.0], # Dummy
                structure_signature={"kind": latest_unit.kind.value} if latest_unit else {},
                origin_context=OriginContext.TEXT,
                confidence=latest_unit.confidence if latest_unit else 0.0,
                entropy=0.0
            )

            # Execute Design Eval
            eval_result = self.design_eval.evaluate(decision_res, memory_state, sem_rep)
            
            # Execute Language Gen
            response_text = self.language_engine.generate(decision_res, memory_state, eval_result, input_text)

            # Log for StateEvaluationEngine
            self._log_execution_data(decision_res, memory_state, eval_result)
            
            # Persist changes
            self.memory_space.canonical.flush()
            self.memory_space.quarantine.flush()

        # 5. Construct Final DTO via Adapter
        return InteractionResultDTO(
            input=input_text,
            response_text=response_text,
            decision=AdapterLayer.to_decision_dto(decision_outcome),
            memory=AdapterLayer.to_memory_dto(latest_unit),
            design_eval=AdapterLayer.to_design_eval_dto(eval_result)
        )

    def get_evaluation_report(self) -> Dict[str, Any]:
        """
        Returns the statistical evaluation report.
        """
        self.state_eval_engine.load_logs(self._execution_logs)
        report = self.state_eval_engine.evaluate()
        return report

    def get_latest_decision(self) -> Optional[InteractionResultDTO]:
        """
        Returns the DTO for the latest decision outcome.
        """
        outcome = None
        if self.vm._state.decision_state.outcomes:
            outcome = self.vm._state.decision_state.outcomes[-1]
        
        # We need a SemanticUnit to create a full InteractionResultDTO or just DecisionDTO
        # For simplicity in 'decision' command, we might just want DecisionDTO.
        # But let's return DecisionDTO wraped or as-is via Adapter.
        return AdapterLayer.to_decision_dto(outcome)

    def get_memory_summary(self) -> Dict[str, Any]:
        """
        Returns a summary of the memory state.
        """
        space = self.memory_space
        canonical_count = len(space.canonical.unit_store.cache)
        quarantine_units = list(space.quarantine.unit_store.cache.values())
        
        quarantine_count = len([u for u in quarantine_units if u.status == MemoryStatus.ACTIVE])
        frozen_count = len([u for u in quarantine_units if u.status == MemoryStatus.FROZEN])
        
        return {
            "canonical_count": canonical_count,
            "quarantine_count": quarantine_count,
            "frozen_exists": frozen_count > 0,
            "frozen_count": frozen_count
        }

    def _log_execution_data(self, decision: DecisionResult, memory: Dict, design_eval: Dict):
        """
        Internal helper to push data to the logs for StateEvaluationEngine
        """
        self._execution_logs["decisions"].append({
            "label": decision.label.value,
            "confidence": decision.confidence,
            "entropy": decision.entropy,
            "utility": decision.utility
        })
        self._execution_logs["memory"].append({
            "store_type": memory.get("store_type"),
            "status": memory.get("status")
        })
        # Recall logs - Phase 3 stuff not fully mocked here yet
        
        self._execution_logs["design_eval"].append(design_eval)
