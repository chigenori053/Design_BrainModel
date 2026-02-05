# design_brain_model/cli/commands/export.py

import typer
import json
import csv
from dataclasses import asdict
from design_brain_model.brain_model.evaluation_interface.ui_api import UiApi

app = typer.Typer()
ui_api = UiApi()

@app.command()
def export(
    format: str = typer.Option("json", "--format", help="Export format (json, csv)"),
    out: str = typer.Option(None, "--out", help="Output file path")
):
    """
    Exports interaction or evaluation data to JSON or CSV.
    """
    # For Phase 19-3a, we export the evaluation report as a representative data set
    report = ui_api.get_evaluation_report()
    
    if format.lower() == "json":
        data = json.dumps(report, indent=2, ensure_ascii=False)
        if out:
            with open(out, "w", encoding="utf-8") as f:
                f.write(data)
            print(f"Exported to {out}")
        else:
            print(data)
            
    elif format.lower() == "csv":
        # Simplified CSV export for the report
        if out:
            with open(out, "w", encoding="utf-8", newline="") as f:
                writer = csv.writer(f)
                # Write flat metrics
                writer.writerow(["Metric", "Value"])
                for k, v in report.items():
                    if isinstance(v, (int, float, str)):
                        writer.writerow([k, v])
            print(f"Exported to {out}")
        else:
            print("CSV output requires --out path")
    else:
        print(f"Unsupported format: {format}")
