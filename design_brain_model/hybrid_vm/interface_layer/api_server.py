# design_brain_model/hybrid_vm/interface_layer/api_server.py
import uvicorn
import logging
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from typing import Any, Dict
from dataclasses import asdict

# --- Domain, ViewModel, and Command Imports ---
# Note: Adjusting relative paths to be robust from the project root.
from design_brain_model.brain_model.memory.space import MemorySpace
from design_brain_model.command import (
    CreateL1AtomCommand,
    CreateL1ClusterCommand,
    ArchiveL1ClusterCommand,
    ConfirmDecisionCommand,
    UpdateDecisionCommand,
)

# Configure Logging
logging.basicConfig(
    filename='server.log',
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# --- FastAPI App and Global Domain Instance ---
app = FastAPI(title="DesignBrainModel API - Phase 17-4")
memory_space = MemorySpace()  # Singleton instance for the application's lifecycle
logger.info("MemorySpace initialized.")

# --- API Models ---
class CommandRequest(BaseModel):
    """Defines the structure for an incoming command request."""
    command_type: str = Field(..., alias="commandType", description="The type of the command to execute.")
    payload: Dict[str, Any] = Field(..., description="The data required to execute the command.")

    class Config:
        populate_by_name = True

# Dictionary to map command type strings from the API to their Python classes.
COMMAND_MAP = {
    "CreateL1Atom": CreateL1AtomCommand,
    "CreateL1Cluster": CreateL1ClusterCommand,
    "ArchiveL1Cluster": ArchiveL1ClusterCommand,
    "ConfirmDecision": ConfirmDecisionCommand,
    "UpdateDecision": UpdateDecisionCommand,
}

# --- Endpoints ---

@app.get("/viewmodel/cluster/{cluster_id}", tags=["ViewModel"])
def get_cluster_viewmodel(cluster_id: str):
    """Retrieves the ViewModel for a specific L1 Cluster."""
    logger.info(f"Requesting ViewModel for cluster: {cluster_id}")
    vm = memory_space.project_to_l1_cluster_vm(cluster_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Cluster with id '{cluster_id}' not found.")
    return asdict(vm)

@app.get("/viewmodel/atom/{atom_id}", tags=["ViewModel"])
def get_atom_viewmodel(atom_id: str):
    """Retrieves the ViewModel for a specific L1 Atom."""
    logger.info(f"Requesting ViewModel for atom: {atom_id}")
    vm = memory_space.project_to_l1_atom_vm(atom_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Atom with id '{atom_id}' not found.")
    return asdict(vm)

@app.get("/viewmodel/decision/{decision_id}", tags=["ViewModel"])
def get_decision_viewmodel(decision_id: str):
    """Retrieves the ViewModel for a specific L2 Decision."""
    logger.info(f"Requesting ViewModel for decision: {decision_id}")
    vm = memory_space.project_to_decision_chip_vm(decision_id)
    if vm is None:
        raise HTTPException(status_code=404, detail=f"Decision with id '{decision_id}' not found.")
    return asdict(vm)

@app.post("/command", tags=["Command"])
def execute_command_endpoint(request: CommandRequest):
    """The sole endpoint for mutating the domain state by executing a command."""
    logger.info(f"Received command: {request.command_type} with payload {request.payload}")
    
    command_class = COMMAND_MAP.get(request.command_type)
    if not command_class:
        raise HTTPException(status_code=400, detail=f"Unknown command type: '{request.command_type}'")
    
    try:
        # Create a command instance from the payload dictionary.
        # This relies on the payload's keys matching the dataclass field names.
        command_instance = command_class(**request.payload)
    except TypeError as e:
        # This can happen if payload keys are wrong or a required key is missing.
        raise HTTPException(status_code=400, detail=f"Invalid payload for '{request.command_type}': {e}")

    try:
        result = memory_space.execute_command(command_instance)
        logger.info(f"Command '{request.command_type}' executed successfully. Result: {result}")
        return {"status": "success", "result": result}
    except (ValueError, TypeError) as e:
        logger.error(f"Command execution failed due to bad request: {e}")
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"An unexpected error occurred during command execution: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="An internal server error occurred.")

if __name__ == "__main__":
    logger.info("Starting DesignBrainModel API Server (Phase 17-4)...")
    # To run this server, execute: `python -m design_brain_model.hybrid_vm.interface_layer.api_server`
    # from the root of the project, ensuring PYTHONPATH is set correctly.
    uvicorn.run("design_brain_model.hybrid_vm.interface_layer.api_server:app", host="0.0.0.0", port=8000, reload=True)
