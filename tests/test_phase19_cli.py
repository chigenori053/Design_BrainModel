import json
import subprocess
import pytest
from pathlib import Path

# Since we useTyper and it can be tested via Runner, 
# but the easiest way to ensure 'real' environment is via subprocess 
# using the installed python in venv.

PYTHON_BIN = "./.venv_phase17/bin/python"
APP_MODULE = "design_brain_model.cli.app"

def run_cli(*args):
    cmd = [PYTHON_BIN, "-m", APP_MODULE] + list(args)
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result

def test_cli_run_json_output():
    """CLI-01: Verify 'run' outputs valid JSON by default."""
    res = run_cli("run", "test input")
    assert res.returncode == 0
    # The output might contain [VM] logs before the JSON
    # We find the last line or the line starting with {
    json_lines = [l for l in res.stdout.splitlines() if l.strip().startswith("{")]
    assert json_lines
    data = json.loads(json_lines[-1])
    assert "input" in data
    assert "response_text" in data
    assert "decision" in data

def test_cli_memory_summary():
    """CLI-02: Verify 'memory' summary output."""
    res = run_cli("memory", "--summary")
    assert res.returncode == 0
    json_lines = [l for l in res.stdout.splitlines() if l.strip().startswith("{")]
    data = json.loads(json_lines[-1])
    assert "canonical_count" in data
    assert "quarantine_count" in data

def test_cli_evaluate_report():
    """CLI-03: Verify 'evaluate' outputs report JSON."""
    res = run_cli("evaluate")
    assert res.returncode == 0
    json_lines = [l for l in res.stdout.splitlines() if l.strip().startswith("{")]
    data = json.loads(json_lines[-1])
    assert "overall_status" in data

def test_cli_export_json(tmp_path):
    """CLI-04: Verify 'export' to file."""
    out_file = tmp_path / "report.json"
    res = run_cli("export", "--format", "json", "--out", str(out_file))
    assert res.returncode == 0
    assert out_file.exists()
    data = json.loads(out_file.read_text())
    assert "overall_status" in data
