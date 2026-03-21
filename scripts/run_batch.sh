#!/usr/bin/env bash
# Operational Phase — Batch Driver
# Usage:
#   ./scripts/run_batch.sh [count] [phase]
#
#   count  : number of runs (default: 10 for Phase A)
#   phase  : a | b | c  (default: a)
#
# Phase A (smoke)  : 10 runs   — crash detection, trace sanity
# Phase B (medium) : 100 runs  — KPI trend, recall effectiveness
# Phase C (long)   : 500 runs  — memory growth, cache behaviour, stability

set -euo pipefail

COUNT="${1:-10}"
PHASE="${2:-a}"
BINARY="${BINARY:-./target/debug/cli}"
LOG_DIR="${LOG_DIR:-logs/phase_${PHASE}}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"

mkdir -p "${LOG_DIR}"

# Workload matrix: vary type and lang to exercise different paths.
TYPES=("web" "api" "cli" "service")
LANGS=("rust" "rust" "rust" "rust")   # extend as needed

PHASE_UPPER="$(echo "${PHASE}" | tr '[:lower:]' '[:upper:]')"
echo "=== Operational Phase ${PHASE_UPPER} — ${COUNT} runs ==="
echo "    binary  : ${BINARY}"
echo "    log dir : ${LOG_DIR}"
echo "    started : ${TIMESTAMP}"
echo ""

SUCCESS=0
FAILURE=0

for i in $(seq 1 "${COUNT}"); do
    IDX=$(( (i - 1) % ${#TYPES[@]} ))
    TYPE="${TYPES[$IDX]}"
    LANG="${LANGS[$IDX]}"
    LOG_FILE="${LOG_DIR}/run_$(printf '%04d' "${i}").json"

    printf "Run %4d/%d  type=%-8s lang=%-6s  →  %s  " \
        "${i}" "${COUNT}" "${TYPE}" "${LANG}" "${LOG_FILE}"

    if "${BINARY}" generate \
        --type "${TYPE}" \
        --lang "${LANG}" \
        --out  "${LOG_FILE}" \
        2>/dev/null; then
        echo "✓"
        SUCCESS=$(( SUCCESS + 1 ))
    else
        echo "✗"
        FAILURE=$(( FAILURE + 1 ))
    fi
done

echo ""
echo "=== Summary ==="
echo "  total   : ${COUNT}"
echo "  success : ${SUCCESS}"
echo "  failure : ${FAILURE}"
if [ "${COUNT}" -gt 0 ]; then
    RATE=$(echo "scale=3; ${SUCCESS} / ${COUNT}" | bc)
    echo "  rate    : ${RATE}"
fi
echo "  logs    : ${LOG_DIR}/"
