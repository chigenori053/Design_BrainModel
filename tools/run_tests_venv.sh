#!/usr/bin/env bash
set -euo pipefail

VENV_DIR=".venv_phase17"
PYTHON_BIN="${VENV_DIR}/bin/python"

if [[ ! -x "${PYTHON_BIN}" ]]; then
  echo "Missing ${PYTHON_BIN}. Create the venv first (e.g., 'uv venv ${VENV_DIR}')." >&2
  exit 1
fi

exec "${PYTHON_BIN}" -m pytest "$@"
