import uuid
from datetime import datetime, timedelta, timezone
from typing import Optional, List, Set, Dict

from pydantic import ValidationError, BaseModel

from design_brain_model.hybrid_vm.control_layer.state import (
    VMState, Message, Role, SemanticUnit, SemanticUnitKind, SemanticUnitStatus,
    HumanOverrideAction, OverrideRecord, DecisionNode, DecisionNodeStatus,
    DecisionNodeCandidate, DecisionNodeSnapshot, ConfidenceLevel, EntropyLevel
)
from design_brain_model.hybrid_vm.events import (
    BaseEvent, EventType, UserInputEvent, ExecutionResultEvent,
    DecisionMadeEvent, ExecutionRequestEvent, HumanOverrideEvent,
    RequestReevaluationEvent, VmTerminateEvent, Actor
)
from design_brain_model.brain_model.api import DesignCommand, DesignCommandType
from design_brain_model.hybrid_vm.interface_layer.services import InterfaceServices
from design_brain_model.hybrid_vm.execution_layer.mock import MockExecutionEngine
from design_brain_model.hybrid_vm.control_layer.decision_pipeline import DecisionPipeline
from design_brain_model.hybrid_vm.control_layer.state import Policy, DecisionCandidate, DecisionOutcome, Evaluation, ConsensusStatus
from design_brain_model.hybrid_vm.control_layer.human_override import HumanOverrideHandler

# --- Phase 17-2: Custom Exceptions for Override Contract ---
class DecisionNotFoundError(Exception):
    pass

class InvalidOverridePayloadError(Exception):
    pass

class HumanOverrideError(Exception):
    pass

class UserInputError(Exception):
    pass

# --- Phase 17-2: Pydantic model for API Payload validation ---
class HumanOverridePayload(BaseModel):
    target_decision_id: str
    override_target_l2: str
    override_action: Optional[HumanOverrideAction] = None
    reason: Optional[str] = None


class HybridVM:
    def __init__(self, vm_id: Optional[str] = None, initial_state: Optional[VMState] = None):
        if vm_id:
            self.vm_id = str(uuid.UUID(vm_id))
        else:
            self.vm_id = str(uuid.uuid4())
        self._state = initial_state or VMState()
        self.interface = InterfaceServices()
        self.execution = MockExecutionEngine()
        self.decision_pipeline = DecisionPipeline()
        self.human_override_handler = HumanOverrideHandler()
        self.event_log: List[BaseEvent] = []
        self.sink_log: List[Dict[str, str]] = []
        self._event_counter = 0
        self._clock_counter = 0
        self._logical_index = 0
        self._entity_counter: Dict[str, int] = {}

    @classmethod
    def create(cls) -> "HybridVM":
        return cls()

    @classmethod
    def from_snapshot(cls, snapshot: dict, vm_id: Optional[str] = None) -> "HybridVM":
        state = VMState.model_validate(snapshot)
        return cls(vm_id=vm_id, initial_state=state)

    # def process_human_override(self, override_action: str, reason: str, target_decision_id: str, candidate_ids: Optional[List[str]] = None, override_event_id: Optional[str] = None) -> DecisionOutcome:
    #     """
    #     DEPRECATED in Phase 17-2. Replaced by _handle_human_override.
    #     """
    #     print(f"[VM] Processing Human Override: {override_action} (Reason: {reason})")
    #     human_eval = self.human_override_handler.create_human_evaluation(
    #         decision=override_action,
    #         reason=reason,
    #         candidate_ids=candidate_ids or [],
    #         timestamp=self._next_timestamp()
    #     )
    #     outcome = DecisionOutcome(
    #         resolves_question_id="override-context",
    #         policy_id=None,
    #         policy_snapshot={},
    #         evaluations=[human_eval],
    #         consensus_status=self._map_override_action(override_action),
    #         lineage=None,
    #         ranked_candidates=[],
    #         explanation="",
    #         human_reason=reason,
    #         override_event_id=override_event_id,
    #         overridden_decision_id=target_decision_id,
    #     )
    #     outcome.explanation = self.decision_pipeline.explanation_generator.generate(outcome)
    #     if not outcome.outcome_id:
    #         outcome.outcome_id = outcome.compute_deterministic_id()
    #     self.process_event(DecisionMadeEvent(
    #         type=EventType.DECISION_MADE,
    #         payload={"outcome": outcome},
    #         actor=Actor.USER
    #     ))
    #     return outcome

    def process_event(self, event: BaseEvent):
        """
        Main Event Loop.
        1. Validate Event
        2. Mutate State
        3. Trigger Side Effects (Brain calls)
        """
        print(f"[VM] Processing Event: {event.type} by {event.actor}")
        if not getattr(event, "event_id", None):
            event.event_id = self._next_event_id()
        if not getattr(event, "vm_id", None):
            event.vm_id = self.vm_id
        if not getattr(event, "logical_index", None):
            event.logical_index = self._next_logical_index()
        if not getattr(event, "wall_timestamp", None):
            event.wall_timestamp = datetime.now(timezone.utc)
        if not getattr(event, "parent_event_id", None) and self.event_log:
            event.parent_event_id = self.event_log[-1].event_id
        self.event_log.append(event)

        if self._is_system_halted() and event.type != EventType.VM_TERMINATE:
            self._sink_event(event, error="System halted after OVERRIDDEN_L2")
            return

        dispatch = {
            EventType.USER_INPUT: self._handle_user_input,
            EventType.EXECUTION_REQUEST: self._handle_execution_request,
            EventType.EXECUTION_RESULT: self._handle_execution_result,
            EventType.DECISION_MADE: self._handle_decision_outcome,
            EventType.HUMAN_OVERRIDE: self._handle_human_override_event,
            EventType.REQUEST_REEVALUATION: self._handle_request_reevaluation,
            EventType.VM_TERMINATE: self._handle_vm_terminate,
        }

        handler = dispatch.get(event.type)
        if handler:
            handler(event)
        else:
            self._sink_event(event, error="Unhandled event type")

    def evaluate_decision(self, question_id: str, candidates: List[DecisionCandidate], policy: Policy):
        """
        Public API to trigger a decision evaluation flow.
        Likely called by the Brain or Manually.
        """
        if self._is_system_halted():
            print("[VM] Decision evaluation blocked after OVERRIDDEN_L2")
            return
        outcome = self.decision_pipeline.process_decision(question_id, candidates, policy)
        
        # Emit the outcome event
        self.process_event(DecisionMadeEvent(
            type=EventType.DECISION_MADE,
            payload={"outcome": outcome},
            actor=Actor.DESIGN_BRAIN
        ))
        
    def _handle_decision_outcome(self, event: BaseEvent):
        outcome_data = event.payload.get("outcome")
        if outcome_data is None:
            self._sink_event(event, error="Missing outcome for decision")
            return
        # Ensure it's a dict or object handling
        if isinstance(outcome_data, dict):
            outcome = DecisionOutcome(**outcome_data)
        else:
            outcome = outcome_data
            
        self._state.decision_state.outcomes.append(outcome.model_copy(deep=True))
        print(f"[VM] Decision Reached for {outcome.resolves_question_id}: {outcome.explanation}")

        decision_node = self._state.decision_state.decision_nodes.get(outcome.resolves_question_id)
        if decision_node and decision_node.status == DecisionNodeStatus.OVERRIDDEN_L2:
            return

        decision_node = decision_node or DecisionNode(id=outcome.resolves_question_id)
        decision_node.all_candidates = [
            DecisionNodeCandidate(candidate_id=c.candidate_id, content=c.content)
            for c in outcome.ranked_candidates
        ]
        decision_node.selected_candidate = decision_node.all_candidates[0] if decision_node.all_candidates else None

        confidence, entropy = self._compute_snapshot_metrics(outcome)
        decision_node.confidence = ConfidenceLevel(confidence)
        decision_node.entropy = EntropyLevel(entropy)

        if outcome.consensus_status in (ConsensusStatus.REVIEW, ConsensusStatus.ESCALATE):
            decision_node.status = DecisionNodeStatus.REVIEW
        else:
            decision_node.status = DecisionNodeStatus.DECIDED

        self._state.decision_state.decision_nodes[decision_node.id] = decision_node

    def _handle_user_input(self, event: BaseEvent):
        action = event.payload.get("action")
        if action == "create_unit":
            unit_data = event.payload.get("unit")
            if not unit_data:
                self._sink_event(event, error="Missing unit data for create_unit")
                return
            self._apply_semantic_unit_created(unit_data, event.event_id)
            return
        if action == "confirm_unit":
            unit_id = event.payload.get("unit_id")
            if not unit_id:
                self._sink_event(event, error="Missing unit_id for confirm_unit")
                return
            self._apply_semantic_unit_confirmed(unit_id, event.event_id)
            return

        content = event.payload.get("content")
        if content is None or (isinstance(content, str) and not content.strip()):
            raise UserInputError("Empty input is not allowed")

        msg_id = self._next_entity_id("msg")
        
        # 1. Update State: Add Message
        new_msg = Message(
            id=msg_id,
            role=Role.USER,
            content=content,
            timestamp=self._next_timestamp()
        )
        self._state.conversation.history.append(new_msg)
        
        # 2. Trigger Brain: Extract Semantics
        cmd = DesignCommand(
            type=DesignCommandType.EXTRACT_SEMANTICS,
            payload={"content": content, "message_id": msg_id}
        )
        result = self.interface.brain.handle_design_command(cmd)
        
        if result.success:
            units_data = result.data.get("units", [])
            for u_data in units_data:
                # Convert raw brain data to SemanticUnit Schema parameters
                # Note: Brain is mock, so we adapt here for Phase 1
                kind_str = u_data.get("type", "requirement").lower()
                # Map old "concept" to "requirement" or similar if needed, or keep strictly validation
                try:
                    kind = SemanticUnitKind(kind_str)
                except ValueError:
                    kind = SemanticUnitKind.REQUIREMENT

                unit_id = self._next_entity_id("unit")
                
                # Create Unit Payload
                payload = {
                    "unit": {
                        "id": unit_id,
                        "kind": kind,
                        "content": u_data.get("content", ""),
                        "status": SemanticUnitStatus.UNSTABLE,
                        "confidence": 1.0, # Mock
                        "origin_event_id": str(uuid.uuid4()), # Placeholder for now
                        "source_message_id": msg_id
                    }
                }
                
                # Directly apply creation within handler to keep event types fixed
                self._apply_semantic_unit_created(payload["unit"], event.event_id)

    def _apply_semantic_unit_created(self, unit_data: Dict, event_id: Optional[str]):
        unit = SemanticUnit(**unit_data)
        if unit.origin_event_id is None:
            unit.origin_event_id = event_id
        unit.last_updated_event_id = event_id
        self._state.semantic_units.units[unit.id] = unit
        print(f"[VM] Created Unit: {unit.content} ({unit.kind}) - Status: {unit.status}")

    def _apply_semantic_unit_confirmed(self, unit_id: str, event_id: Optional[str]):
        unit = self._state.semantic_units.units.get(unit_id)
        
        if not unit:
            print(f"[VM] Error: Unit {unit_id} not found.")
            return

        # Terminal State Safety: locked states
        if unit.status in [SemanticUnitStatus.STABLE, SemanticUnitStatus.REJECTED]:
            print(f"[VM] No-Op: Unit {unit.id} is already in terminal state {unit.status}.")
            return

        # State Transition Logic
        current_status = unit.status
        next_status = None
        
        if current_status == SemanticUnitStatus.UNSTABLE:
            next_status = SemanticUnitStatus.REVIEW
        elif current_status == SemanticUnitStatus.REVIEW:
            next_status = SemanticUnitStatus.STABLE
        else:
             # Should be unreachable if initial states are correct, but safe fallback
            print(f"[VM] No transition allowed from {current_status}")
            return

        # Check Conflicts before transition
        conflicts = self._check_conflicts(unit, next_status)
        if conflicts:
            for c in conflicts:
                self._sink_event(BaseEvent(type=EventType.USER_INPUT, payload=c, actor=Actor.EXECUTION_LAYER), error="Semantic conflict")
            return

        # Apply Transition
        old_status = unit.status
        unit.status = next_status
        unit.last_updated_event_id = event_id
        print(f"[VM] Transitioned Unit {unit.id}: {old_status} -> {unit.status}")

    def _check_conflicts(self, unit: SemanticUnit, target_status: SemanticUnitStatus) -> List[Dict]:
        conflicts = []
        
        # 1. Dependency Violation
        if target_status == SemanticUnitStatus.STABLE:
            for dep_id in unit.dependencies:
                dep_unit = self._state.semantic_units.units.get(dep_id)
                # Dependency must be STABLE. If not found or not stable, it's a violation.
                if not dep_unit or dep_unit.status != SemanticUnitStatus.STABLE:
                    conflicts.append({
                        "conflict_type": "Dependency Violation",
                        "unit_id": unit.id,
                        "dependency_id": dep_id,
                        "reason": f"Dependency {dep_id} is not Stable"
                    })

        # 2. Multi-Decision Conflict (Stub)
        if unit.kind == SemanticUnitKind.DECISION and target_status == SemanticUnitStatus.STABLE:
            pass

        return conflicts

    def _sink_event(self, event: BaseEvent, error: str):
        self.sink_log.append({"event_id": event.event_id or "", "type": event.type.value, "error": error})
        print(f"[VM] EVENT SINK: {error} -> {event.type}")


    def _handle_execution_request(self, event: BaseEvent):
        print("[VM] Requesting Simulation...")
        self._state.simulation.is_running = True
        
        # Call Execution Layer
        success, message, error_type = self.execution.run_system(
            self._state.system_structure.model_dump()
        )
        
        # Emit Result Event (Self-loop)
        result_payload = {
            "success": success,
            "error": message if not success else None,
            "success_message": message if success else None,
            "error_type": error_type
        }
        self.process_event(ExecutionResultEvent(type=EventType.EXECUTION_RESULT, payload=result_payload))
        self._state.simulation.is_running = False

    def _handle_execution_result(self, event: BaseEvent):
        success = event.payload.get("success")
        message = event.payload.get("error") if not success else None
        if success:
            message = event.payload.get("success_message")
        error_type = event.payload.get("error_type")

        self._state.simulation.last_result = message
        if not success:
            self._state.execution_feedback.last_error = message
            self._state.execution_feedback.error_type = error_type
            print(f"[VM] Execution Failed: {message} ({error_type})")
        else:
            print(f"[VM] Execution Success: {message}")

    def get_state_snapshot(self) -> dict:
        return self._state.model_copy(deep=True).model_dump(mode='json')

    def build_snapshot(self) -> dict:
        state = self.get_state_snapshot()
        # Use actual objects for computation
        outcomes_objs = self._state.decision_state.outcomes
        last_obj = outcomes_objs[-1] if outcomes_objs else None
        
        confidence, entropy = self._compute_snapshot_metrics(last_obj)
        
        outcomes_data = state.get("decision_state", {}).get("outcomes", [])
        last_data = outcomes_data[-1] if outcomes_data else None
        
        return {
            "snapshot_id": str(uuid.uuid4()),
            "vm_state": state,
            "decision_history": outcomes_data,
            "current_decision_id": last_data.get("outcome_id") if last_data else None,
            "confidence": confidence,
            "entropy": entropy,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        }

    def get_explanation_snapshot(self) -> dict:
        return {
            "state": self.get_state_snapshot(),
            "events": [e.model_dump() for e in self.event_log],
        }

    def terminate(self) -> dict:
        self.process_event(VmTerminateEvent(type=EventType.VM_TERMINATE, payload={}, actor=Actor.USER))
        return self.get_state_snapshot()

    def _next_event_id(self) -> str:
        self._event_counter += 1
        return str(uuid.uuid5(uuid.UUID(self.vm_id), f"event:{self._event_counter}"))

    def _next_timestamp(self) -> datetime:
        self._clock_counter += 1
        return datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(microseconds=self._clock_counter)

    def _next_logical_index(self) -> int:
        self._logical_index += 1
        return self._logical_index

    def _next_entity_id(self, prefix: str) -> str:
        self._entity_counter[prefix] = self._entity_counter.get(prefix, 0) + 1
        return f"{self.vm_id}:{prefix}:{self._entity_counter[prefix]}"

    def _compute_snapshot_metrics(self, outcome: Optional[DecisionOutcome]) -> tuple[str, str]:
        if not outcome or not outcome.evaluations:
            return ("LOW", "HIGH")
        avg_conf = sum(e.confidence for e in outcome.evaluations) / len(outcome.evaluations)
        avg_entropy = sum(e.entropy for e in outcome.evaluations) / len(outcome.evaluations)
        if avg_conf > 0.8:
            confidence = "HIGH"
        elif avg_conf < 0.4:
            confidence = "LOW"
        else:
            confidence = "MID"
        if avg_entropy > 0.6:
            entropy = "HIGH"
        elif avg_entropy < 0.3:
            entropy = "LOW"
        else:
            entropy = "MID"
        return (confidence, entropy)

    def _handle_human_override_event(self, event: BaseEvent):
        """
        Handles the HUMAN_OVERRIDE event according to the Phase 17-2 contract.
        This method now propagates exceptions to be handled by the API layer.
        """
        # The state is modified in place by the handler.
        # Exceptions are raised to the caller (e.g., API server) for proper error response.
        self._handle_human_override(event.payload, event.event_id)

    def _handle_human_override(self, payload_data: dict, event_id: str):
        """
        Implements the logic for Phase 17-2 Human Override contract.
        Modifies the state in place.
        """
        # 1. Validate Payload
        try:
            payload = HumanOverridePayload(**payload_data)
        except ValidationError as e:
            raise InvalidOverridePayloadError(str(e)) from e

        # 2. Find Target Decision Node
        decision_node = self._state.decision_state.decision_nodes.get(payload.target_decision_id)
        if not decision_node:
            raise DecisionNotFoundError(f"Decision node with ID {payload.target_decision_id} not found.")

        if not payload.override_target_l2:
            raise HumanOverrideError("override_target_l2 is required")

        if decision_node.status == DecisionNodeStatus.OVERRIDDEN_L2:
            raise HumanOverrideError("Decision node already overridden")

        # 3. Snapshot before override
        decision_node.snapshot_before_override = self._build_decision_node_snapshot(decision_node)

        # 4. Apply Override (freeze confidence/entropy)
        decision_node.human_override = True
        decision_node.override_target_l2 = payload.override_target_l2
        decision_node.status = DecisionNodeStatus.OVERRIDDEN_L2

        # 5. Snapshot after override
        decision_node.snapshot_after_override = self._build_decision_node_snapshot(decision_node)

        # 6. Record Override in History (legacy audit trail)
        original_consensus = ConsensusStatus.REVIEW
        for outcome in self._state.decision_state.outcomes:
            if outcome.resolves_question_id == payload.target_decision_id:
                original_consensus = outcome.consensus_status or ConsensusStatus.REVIEW
                break

        record = OverrideRecord(
            decision_id=payload.target_decision_id,
            original_status=original_consensus,
            override_status=payload.override_action or HumanOverrideAction.REVIEW,
            reason=payload.reason
        )
        self._state.override_history.append(record)

        print(f"[VM] Human Override Applied to {decision_node.id}. Status: {decision_node.status}")


    def _handle_request_reevaluation(self, event: BaseEvent):
        if self._state.decision_state.outcomes:
            last = self._state.decision_state.outcomes[-1]
            if last.override_event_id or last.overridden_decision_id:
                self._sink_event(event, error="Reevaluation blocked after human override")
                return
        self._sink_event(event, error="Reevaluation not implemented in Phase13")

    def _handle_vm_terminate(self, event: BaseEvent):
        print(f"[VM] Terminate requested for vm_id={self.vm_id}")

    def _is_system_halted(self) -> bool:
        return any(
            node.status == DecisionNodeStatus.OVERRIDDEN_L2
            for node in self._state.decision_state.decision_nodes.values()
        )

    def _build_decision_node_snapshot(self, decision_node: DecisionNode) -> DecisionNodeSnapshot:
        return DecisionNodeSnapshot(
            decision_node_id=decision_node.id,
            all_candidates=list(decision_node.all_candidates),
            selected_candidate=decision_node.selected_candidate,
            confidence=decision_node.confidence,
            entropy=decision_node.entropy,
            timestamp=self._next_timestamp(),
            system_version="Phase17"
        )

    # def _map_override_action(self, action: str) -> ConsensusStatus:
    #     DEPRECATED in Phase 17-2
    #     if action == "ACCEPT":
    #         return ConsensusStatus.ACCEPT
    #     if action == "REJECT":
    #         return ConsensusStatus.REJECT
    #     if action == "FORCE_REVIEW":
    #         return ConsensusStatus.REVIEW
    #     return ConsensusStatus.REVIEW
