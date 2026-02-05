# design_brain_model/cli/commands/evaluate.py

import typer
import json
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi

app = typer.Typer()
ui_api = UiApi()

@app.command()
def evaluate(
    logs: str = typer.Option(None, "--logs", help="Path to logs (simulated in Phase 19-3a)"),
    pretty: bool = typer.Option(False, "--pretty", help="Human-readable output")
):
    """
    Displays the result of the StateEvaluationEngine.
    """
    # UiApi.get_evaluation_report() returns a Dict
    report = ui_api.get_evaluation_report()
    
    if pretty:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        print(json.dumps(report, ensure_ascii=False))
