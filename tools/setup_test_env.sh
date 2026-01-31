#!/usr/bin/env bash
set -euo pipefail

VENV_DIR=".venv_phase17"
PYTHON_BIN="${VENV_DIR}/bin/python"

if [[ ! -x "${PYTHON_BIN}" ]]; then
  uv venv "${VENV_DIR}"
fi

uv pip install --python "${PYTHON_BIN}" fastapi pytest httpx uvicorn numpy

echo "Ready. Run tests with: tools/run_tests_venv.sh"
