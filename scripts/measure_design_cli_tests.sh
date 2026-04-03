#!/usr/bin/env bash
set -euo pipefail

limit="${1:-20}"
tmp_file="$(mktemp)"
trap 'rm -f "${tmp_file}"' EXIT

mapfile -t tests < <(cargo test -p design_cli -- --list | awk '/: test$/ {print $1}')

for test_name in "${tests[@]}"; do
  start="$(python3 - <<'PY'
import time
print(time.time())
PY
)"
  cargo test -p design_cli "${test_name}" -- --exact --test-threads=1 >/dev/null
  end="$(python3 - <<'PY'
import time
print(time.time())
PY
)"
  python3 - "${start}" "${end}" "${test_name}" >>"${tmp_file}" <<'PY'
import sys
start = float(sys.argv[1])
end = float(sys.argv[2])
name = sys.argv[3]
print(f"{end - start:.3f}\t{name}")
PY
done

sort -rn "${tmp_file}" | head -n "${limit}"
