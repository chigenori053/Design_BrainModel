import uuid
from datetime import datetime, timedelta, timezone
from typing import Optional, List, Set, Dict

from design_brain_model.hybrid_vm.control_layer.state import VMState, Message, Role, SemanticUnit, SemanticUnitKind, SemanticUnitStatus
from design_brain_model.hybrid_vm.events import (
    BaseEvent, EventType, UserInputEvent, ExecutionResultEvent, 
    SemanticUnitCreatedEvent, SemanticUnitConfirmedEvent, SemanticConflictDetectedEvent,
    DecisionOutcomeGeneratedEvent,
    Actor
)
from design_brain_model.brain_model.api import DesignCommand, DesignCommandType
from design_brain_model.hybrid_vm.interface_layer.services import InterfaceServices
from design_brain_model.hybrid_vm.execution_layer.mock import MockExecutionEngine
from design_brain_model.hybrid_vm.control_layer.decision_pipeline import DecisionPipeline
from design_brain_model.hybrid_vm.control_layer.state import Policy, DecisionCandidate, DecisionOutcome, Evaluation, ConsensusStatus
from design_brain_model.hybrid_vm.control_layer.human_override import HumanOverrideHandler

class HybridVM:
    def __init__(self, vm_id: Optional[str] = None, initial_state: Optional[VMState] = None):
        self.vm_id = vm_id or str(uuid.uuid4())
        self._state = initial_state or VMState()
        self.interface = InterfaceServices()
        self.execution = MockExecutionEngine()
        self.decision_pipeline = DecisionPipeline()
        self.human_override_handler = HumanOverrideHandler()
        self.event_log: List[BaseEvent] = []
        self._event_counter = 0
        self._clock_counter = 0
        self._entity_counter: Dict[str, int] = {}

    @classmethod
    def create(cls) -> "HybridVM":
        return cls()

    @classmethod
    def from_snapshot(cls, snapshot: dict, vm_id: Optional[str] = None) -> "HybridVM":
        state = VMState.model_validate(snapshot)
        return cls(vm_id=vm_id, initial_state=state)

    def process_human_override(self, decision: str, reason: str, candidate_ids: Optional[List[str]] = None) -> DecisionOutcome:
        """
        Special Entry Point for Human Override (Evaluation Injection).
        Treats human input as a 100% confidence evaluation.
        """
        print(f"[VM] Processing Human Override: {decision} (Reason: {reason})")
        
        # 1. Create Human Evaluation via Handler
        human_eval = self.human_override_handler.create_human_evaluation(
            decision=decision,
            reason=reason,
            candidate_ids=candidate_ids or [],
            timestamp=self._next_timestamp()
        )
        
        # 2. Re-run Pipeline with External Evaluation
        # Note: In a real scenario, we might need context (question_id, candidates).
        # For Phase 7, we Mock/Reuse the last decision context or generic one if empty.
        # This assumes the pipeline can handle just evaluations or we reconstruct context.
        # Simplification: We wrap it effectively.
        
        # Creating dummy context if needed or relying on pipeline defaults
        # Ideally pipeline.process_decision handles overrides.
        
        outcome = self.decision_pipeline.process_decision(
            question_id="override-context", 
            candidates=[], 
            policy=None,
            external_evaluations=[human_eval]
        )
        
        # Force Override Values onto Outcome
        # The pipeline calculates consensus based on utils, but Human Override 
        # is often an explicit forcing function regardless of utility math.
        outcome.human_reason = reason
        # Map decision string (ACCEPT/REJECT) to ConsensusStatus
        try:
            outcome.consensus_status = ConsensusStatus(decision)
        except ValueError:
            # Fallback if mapped incorrectly, though Enum should handle standard strings
            pass
            
        # Regenerate Explanation to reflect Override
        outcome.explanation = self.decision_pipeline.explanation_generator.generate(outcome)
            
        # 3. Emit Result
        self.process_event(DecisionOutcomeGeneratedEvent(
            type=EventType.DECISION_OUTCOME_GENERATED,
            payload={"outcome": outcome},
            actor=Actor.USER
        ))
        
        return outcome

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
        self.event_log.append(event)
        
        if event.type == EventType.USER_INPUT:
            self._handle_user_input(event)
        elif event.type == EventType.SEMANTIC_UNIT_CREATED:
            self._handle_semantic_unit_created(event)
        elif event.type == EventType.SEMANTIC_UNIT_CONFIRMED:
            self._handle_semantic_unit_confirmed(event)
        elif event.type == EventType.SEMANTIC_CONFLICT_DETECTED:
             self._handle_conflict_detected(event)
        elif event.type == EventType.SIMULATION_REQUEST:
            self._handle_simulation_request(event)
        elif event.type == EventType.DECISION_OUTCOME_GENERATED:
            self._handle_decision_outcome(event)
        elif event.type == EventType.EXECUTION_RESULT:
            self._handle_execution_result(event)
        # ... handle other events

    def evaluate_decision(self, question_id: str, candidates: List[DecisionCandidate], policy: Policy):
        """
        Public API to trigger a decision evaluation flow.
        Likely called by the Brain or Manually.
        """
        outcome = self.decision_pipeline.process_decision(question_id, candidates, policy)
        
        # Emit the outcome event
        self.process_event(DecisionOutcomeGeneratedEvent(
            type=EventType.DECISION_OUTCOME_GENERATED,
            payload={"outcome": outcome},
            actor=Actor.DESIGN_BRAIN
        ))
        
    def _handle_decision_outcome(self, event: BaseEvent):
        outcome_data = event.payload.get("outcome")
        # Ensure it's a dict or object handling
        if isinstance(outcome_data, dict):
            outcome = DecisionOutcome(**outcome_data)
        else:
            outcome = outcome_data
            
        self._state.decision_state.outcomes.append(outcome.model_copy(deep=True))
        print(f"[VM] Decision Reached for {outcome.resolves_question_id}: {outcome.explanation}")

    def _handle_user_input(self, event: BaseEvent):
        content = event.payload.get("content")
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
                
                # Emit Creation Event
                self.process_event(SemanticUnitCreatedEvent(
                    type=EventType.SEMANTIC_UNIT_CREATED,
                    payload=payload,
                    actor=Actor.DESIGN_BRAIN
                ))

    def _handle_semantic_unit_created(self, event: BaseEvent):
        unit_data = event.payload.get("unit")
        unit = SemanticUnit(**unit_data)
        if unit.origin_event_id is None:
            unit.origin_event_id = event.event_id
        unit.last_updated_event_id = event.event_id
        self._state.semantic_units.units[unit.id] = unit
        print(f"[VM] Created Unit: {unit.content} ({unit.kind}) - Status: {unit.status}")

    def _handle_semantic_unit_confirmed(self, event: BaseEvent):
        unit_id = event.payload.get("unit_id")
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
                 self.process_event(SemanticConflictDetectedEvent(
                     type=EventType.SEMANTIC_CONFLICT_DETECTED,
                     payload=c,
                     actor=Actor.EXECUTION_LAYER 
                 ))
            return

        # Apply Transition
        old_status = unit.status
        unit.status = next_status
        unit.last_updated_event_id = event.event_id
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

    def _handle_conflict_detected(self, event: BaseEvent):
        payload = event.payload
        # Optional: Append to an explicit audit log if we add one to VMState
        # For now, print to console as per "read-only and intended for debugging"
        print(f"[VM] CONFLICT DETECTED: {payload.get('conflict_type')} - {payload.get('reason')}")


    def _handle_simulation_request(self, event: BaseEvent):
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
        self.process_event(BaseEvent(type=EventType.EXECUTION_RESULT, payload=result_payload))
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
        return self._state.model_copy(deep=True).model_dump()

    def _next_event_id(self) -> str:
        self._event_counter += 1
        return f"{self.vm_id}:{self._event_counter}"

    def _next_timestamp(self) -> datetime:
        self._clock_counter += 1
        return datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(microseconds=self._clock_counter)

    def _next_entity_id(self, prefix: str) -> str:
        self._entity_counter[prefix] = self._entity_counter.get(prefix, 0) + 1
        return f"{self.vm_id}:{prefix}:{self._entity_counter[prefix]}"
