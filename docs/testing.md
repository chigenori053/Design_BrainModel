# Testing Guide

## Phase1 fast suite (local)

Run only the Phase1 target crates for quick feedback:

```bash
time cargo test -p agent_core
time cargo test -p design_cli
```

This is the default local path (`cargo test`) and excludes heavy tests by default.

## Heavy tests

Heavy tests are behind the `ci-heavy` feature.

- `agent_core`: long stability/determinism paths
- `design_cli`: heavy command flow paths

Run heavy tests explicitly:

```bash
cargo test -p agent_core --features ci-heavy
cargo test -p design_cli --features ci-heavy
```

## Full CI suite

CI runs all features enabled:

```bash
cargo test --all-features
```
