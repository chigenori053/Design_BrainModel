# Code Review Remediation Report

## Targeted Risks
- Availability: unbounded cache growth and unbounded CLI inputs
- Spec drift: options present but behavior not wired
- Maintainability: duplicated soft-trace/soft-bench candidate scoring logic

## Remediation Plan
1. Add strict CLI input validation and upper bounds.
2. Introduce bounded field cache to prevent unbounded memory growth.
3. Wire `--log-per-depth` and `--field-profile` to concrete behavior.
4. Extract duplicated soft candidate generation/scoring into shared helper.
5. Rebuild and run stability tests.

## Implemented Changes

### 1) Availability hardening
- Added hard limits in CLI:
  - `MAX_DEPTH=1000`
  - `MAX_BEAM=100`
  - `MAX_BENCH_ITER=1000`
- Added numeric finite/range checks for:
  - `category-alpha`, `temperature`, `entropy-beta`
  - `lambda-min`, `lambda-target-entropy`, `lambda-k`, `lambda-ema`
- File:
  - `apps/cli/src/main.rs`

### 2) Bounded cache
- Added bounded projection cache with eviction order:
  - `FIELD_CACHE_CAPACITY=50_000`
  - `bounded_cache_get_or_insert(...)`
- Applied to soft trace and soft bench paths.
- File:
  - `crates/agent_core/src/lib.rs`

### 3) Spec option behavior alignment
- `--log-per-depth` now controls trace output shape:
  - enabled: all depth rows
  - disabled: final depth row only
- `--field-profile` now gates field timing accumulation.
- Files:
  - `apps/cli/src/main.rs`
  - `crates/agent_core/src/lib.rs`

### 4) Maintainability improvements
- Introduced shared soft-candidate builder to reduce duplicated logic:
  - `SoftCandidateBatch`
  - `build_soft_candidates_for_frontier(...)`
- Reused by both soft trace and soft bench paths.
- File:
  - `crates/agent_core/src/lib.rs`

## Validation
- Build:
  - `cargo build -p design_cli` ✅
- Stability tests:
  - `cargo test -p agent_core --test stability_tests` ✅ (9/9)

## Notes
- The previous unbounded cache path was replaced by bounded eviction.
- CLI now fails fast on invalid/unsafe parameter values.
- Soft-mode shared logic reduces future divergence risk between trace/bench implementations.
