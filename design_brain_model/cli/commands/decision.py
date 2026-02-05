# design_brain_model/cli/commands/decision.py

import typer
import json
from dataclasses import asdict
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi

app = typer.Typer()
ui_api = UiApi()

@app.command()
def decision(
    latest: bool = typer.Option(True, "--latest", help="Show the latest decision outcome"),
    pretty: bool = typer.Option(False, "--pretty", help="Human-readable output")
):
    """
    Retrieves the latest decision outcome via UiApi.
    """
    dto = ui_api.get_latest_decision()
    
    output = asdict(dto) if dto else {"message": "No decision available."}
    
    if pretty:
        print(json.dumps(output, indent=2, ensure_ascii=False))
    else:
        print(json.dumps(output, ensure_ascii=False))
