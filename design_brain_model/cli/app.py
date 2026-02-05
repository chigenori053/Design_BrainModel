# design_brain_model/cli/app.py

import typer
from design_brain_model.cli.commands.run import run
from design_brain_model.cli.commands.decision import decision
from design_brain_model.cli.commands.memory import memory
from design_brain_model.cli.commands.evaluate import evaluate
from design_brain_model.cli.commands.export import export

app = typer.Typer(help="DesignBrainModel CLI - Phase 19-3a Observation Tool")

app.command()(run)
app.command()(decision)
app.command()(memory)
app.command()(evaluate)
app.command()(export)

def main():
    app()

if __name__ == "__main__":
    main()
