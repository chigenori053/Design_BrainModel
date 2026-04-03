# Testing Guide

## DBM PR Gate

Use the PR gate for the default local smoke path and pull request validation:

```bash
bash scripts/run_dbm_test_suite.sh pr
```

This gate runs:

- `contract`
- `integration`
- `resource_release`
- `safety`
- targeted git / gh safety unit tests

## DBM Nightly Gate

Use nightly for exhaustive coverage and cross-workspace validation:

```bash
bash scripts/run_dbm_test_suite.sh nightly
```

This gate adds:

- `nightly_exhaustive`
- `regression`
- `cargo test --workspace -- --test-threads=1`

## DBM Release Gate

Use release gate before tags or GitHub Release publication:

```bash
bash scripts/run_dbm_test_suite.sh release
```

This gate runs:

- `cargo test --workspace --release`
- strict contract checks
- rollback regression
- git / gh remote dry-run representative coverage

## Slow Test Measurement

To extract slow `design_cli` tests for consolidation work:

```bash
bash scripts/measure_design_cli_tests.sh
```
