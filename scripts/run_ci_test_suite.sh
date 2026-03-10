#!/usr/bin/env bash
set -euo pipefail

category="${1:-all}"
shift || true
extra_args=("$@")

run_cargo_test() {
  if ((${#extra_args[@]})); then
    cargo test "$@" "${extra_args[@]}"
  else
    cargo test "$@"
  fi
}

run_invariants() {
  run_cargo_test -p architecture_domain --test invariants --locked
  run_cargo_test -p design_search_engine --test invariants --locked
}

run_engine() {
  run_cargo_test -p design_search_engine --test engine --locked
  run_cargo_test -p evaluation_engine --test engine --locked
  run_cargo_test -p memory_graph --test memory --locked
}

run_determinism() {
  run_cargo_test -p design_search_engine --test determinism --locked -- --test-threads=1
  run_cargo_test -p ai_context --test determinism --locked -- --test-threads=1
  run_cargo_test -p runtime_vm --test determinism --locked -- --test-threads=1
}

run_integration() {
  run_cargo_test -p runtime_vm --test integration --locked
  run_cargo_test -p phase1_integration_tests --test concept_pipeline --locked
  run_cargo_test -p phase1_integration_tests --test canonicalization --locked
  run_cargo_test -p phase1_integration_tests --test reasoning_pipeline --locked
}

run_experiments() {
  run_cargo_test -p design_search_engine --test experiments --locked -- --ignored
}

case "${category}" in
  invariants)
    run_invariants
    ;;
  engine)
    run_engine
    ;;
  determinism)
    run_determinism
    ;;
  integration)
    run_integration
    ;;
  experiments)
    run_experiments
    ;;
  all)
    run_invariants
    run_engine
    run_determinism
    run_integration
    ;;
  *)
    echo "unknown category: ${category}" >&2
    exit 1
    ;;
esac
