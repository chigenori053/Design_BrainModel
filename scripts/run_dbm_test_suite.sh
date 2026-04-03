#!/usr/bin/env bash
set -euo pipefail

gate="${1:-pr}"
shift || true
extra_args=("$@")

run_test() {
  if ((${#extra_args[@]})); then
    cargo test "$@" "${extra_args[@]}" -- --test-threads=1
  else
    cargo test "$@" -- --test-threads=1
  fi
}

run_test_nocapture() {
  if ((${#extra_args[@]})); then
    cargo test "$@" "${extra_args[@]}" -- --nocapture --test-threads=1
  else
    cargo test "$@" -- --nocapture --test-threads=1
  fi
}

apply_heavy_limits() {
  export RUST_TEST_THREADS=1
  export RAYON_NUM_THREADS=2
  export TOKIO_WORKER_THREADS=2
}

run_pr_gate() {
  run_test -p design_cli --test contract --locked
  run_test -p design_cli --test integration --locked
  run_test_nocapture -p design_cli --test resource_release --locked
  run_test -p design_cli --test safety --locked
  run_test -p design_cli git_command_classifier_matches_phase1_rules --locked
  run_test -p design_cli remote_guard_rejects_protected_push_and_dangerous_gh_commands --locked
  run_test -p design_cli dangerous_git_add_dot_is_rejected --locked
}

run_nightly_gate() {
  apply_heavy_limits
  run_pr_gate
  run_test -p design_cli --test nightly_exhaustive --locked
  cargo test --workspace --locked -- --test-threads=1
}

run_release_gate() {
  apply_heavy_limits
  cargo test --workspace --release --locked -- --test-threads=1
  run_test -p design_cli --release --test contract --features contract_strict --locked
  run_test -p design_cli --release rollback_restores_original_file_after_post_commit_failure --locked
  run_test -p design_cli --release remote_integration_pushes_and_creates_pr --locked
}

case "${gate}" in
  pr)
    run_pr_gate
    ;;
  nightly)
    run_nightly_gate
    ;;
  release)
    run_release_gate
    ;;
  *)
    echo "unknown gate: ${gate}" >&2
    exit 1
    ;;
esac
