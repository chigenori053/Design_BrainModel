import uuid
from datetime import datetime
from typing import Optional

from hybrid_vm.state import VMState, Message, Role, SemanticUnit
from hybrid_vm.events import BaseEvent, EventType, UserInputEvent, ExecutionResultEvent
from design_brain.api import DesignBrainModel, DesignCommand, DesignCommandType
from execution_layer.mock import MockExecutionEngine

class HybridVM:
    def __init__(self):
        self.state = VMState()
        self.brain = DesignBrainModel()
        self.execution = MockExecutionEngine()

    def process_event(self, event: BaseEvent):
        """
        Main Event Loop.
        1. Validate Event
        2. Mutate State
        3. Trigger Side Effects (Brain calls)
        """
        print(f"[VM] Processing Event: {event.type}")
        
        if event.type == EventType.USER_INPUT:
            self._handle_user_input(event)
        elif event.type == EventType.SIMULATION_REQUEST:
            self._handle_simulation_request(event)
        elif event.type == EventType.SEMANTIC_EXTRACT:
            # Usually internal, but can be external triggers too
            pass
        elif event.type == EventType.STRUCTURE_EDIT:
            pass
        # ... handle other events

    def _handle_user_input(self, event: BaseEvent):
        content = event.payload.get("content")
        msg_id = str(uuid.uuid4())
        
        # 1. Update State: Add Message
        new_msg = Message(
            id=msg_id,
            role=Role.USER,
            content=content,
            timestamp=datetime.now()
        )
        self.state.conversation.history.append(new_msg)
        
        # 2. Trigger Brain: Extract Semantics
        cmd = DesignCommand(
            type=DesignCommandType.EXTRACT_SEMANTICS,
            payload={"content": content, "message_id": msg_id}
        )
        result = self.brain.handle_design_command(cmd)
        
        if result.success:
            units_data = result.data.get("units", [])
            for u_data in units_data:
                unit = SemanticUnit(**u_data)
                self.state.semantic_units.units[unit.id] = unit
                print(f"[VM] Extracted Unit: {unit.content} ({unit.type})")

    def _handle_simulation_request(self, event: BaseEvent):
        print("[VM] Requesting Simulation...")
        self.state.simulation.is_running = True
        
        # Call Execution Layer
        success, message, error_type = self.execution.run_system(
            self.state.system_structure.model_dump()
        )
        
        # Emit Result Event (Self-loop)
        result_payload = {
            "success": success,
            "error": message if not success else None,
            "error_type": error_type
        }
        self.process_event(BaseEvent(type=EventType.EXECUTION_RESULT, payload=result_payload))

        # Update State
        self.state.simulation.is_running = False
        self.state.simulation.last_result = message
        
        if not success:
            self.state.execution_feedback.last_error = message
            self.state.execution_feedback.error_type = error_type
            print(f"[VM] Execution Failed: {message} ({error_type})")
        else:
            print(f"[VM] Execution Success: {message}")

    def get_state_snapshot(self) -> dict:
        return self.state.model_dump()
