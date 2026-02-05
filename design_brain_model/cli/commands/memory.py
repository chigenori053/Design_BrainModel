# design_brain_model/cli/commands/memory.py

import typer
import json
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi
from design_brain_model.brain_model.memory.types import MemoryStatus

app = typer.Typer()
ui_api = UiApi()

@app.command()
def memory(
    summary: bool = typer.Option(True, "--summary", help="Show memory state summary"),
    pretty: bool = typer.Option(False, "--pretty", help="Human-readable output")
):
    """
    Retrieves a snapshot of the memory state summary.
    """
    # Use official UiApi method
    output = ui_api.get_memory_summary()
    
    if pretty:
        print(json.dumps(output, indent=2, ensure_ascii=False))
    else:
        print(json.dumps(output, ensure_ascii=False))
