# design_brain_model/cli/commands/run.py

import typer
import json
from dataclasses import asdict
from typing import Optional
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi

app = typer.Typer()
ui_api = UiApi()

@app.command()
def run(
    input_text: str,
    show_text: bool = typer.Option(False, "--show-text", help="Display the response text explicitly"),
    pretty: bool = typer.Option(False, "--pretty", help="Human-readable output (JSON default)")
):
    """
    Executes a single interaction and displays the result in JSON format.
    """
    result = ui_api.run_interaction(input_text)
    
    # InteractionResultDTO is a dataclass
    output = asdict(result)
    
    if pretty:
        print(json.dumps(output, indent=2, ensure_ascii=False))
    else:
        # Default JSON output for machines
        print(json.dumps(output, ensure_ascii=False))

    if show_text:
        resp_text = output.get("response_text", "")
        if pretty:
             print(f"\nResponse: {resp_text}")
        else:
             # If not pretty, just append it if needed, but JSON is the priority.
             # According to spec, show-text should show response_text.
             # We can print it to stderr to keep stdout clean for JSON piping, 
             # but the spec doesn't specify. We'll just print it.
             print(f"\nResponse: {resp_text}")
