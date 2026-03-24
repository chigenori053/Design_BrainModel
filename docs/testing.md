# Testing Guide

## Phase1 fast suite (local)

Run the default deterministic path. This excludes heavy tests by default:

```bash
cargo test-fast
cargo test-architecture
time cargo test -p runtime_vm -- --test-threads=1
time cargo test -p agent_core -- --test-threads=1
time cargo test -p design_cli -- --test-threads=1
```

Fast tests should stay in the default `#[test]` set:

- unit
- architecture / boundary enforcement
- contract tests
- minimum integration coverage

## Heavy tests

Heavy tests are isolated behind `#[ignore]`.

- long-running integration
- memory growth / stress
- quality / reasoning suites

Run heavy tests explicitly:

```bash
cargo test-runtime-heavy
cargo test-stress
cargo test -p runtime_vm -- --ignored --test-threads=1
cargo test -p agent_core --test heavy --release --features ci-heavy -- --test-threads=1
```

## Split categories

Use split execution when a single `cargo test` is too expensive:

```bash
cargo test-architecture
cargo test-invariants
cargo test-engine
cargo test-knowledge
cargo test-contract
cargo test-determinism
cargo test-integration
cargo test-all-split
```

Or route through one entrypoint:

```bash
cargo xtest fast
cargo xtest runtime-heavy
cargo xtest experiments
```

## Full CI suite

CI runs fast tests first, then ignored heavy tests, both single-threaded:

```bash
cargo test --workspace -- --test-threads=1
cargo test --workspace -- --ignored --test-threads=1
```
