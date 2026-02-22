# TensorEngine Interface Refactor
Version: 1.1
Status: approved

---

## Purpose
Separate phase encoding logic from TensorEngine core execution flow to reduce coupling and keep public API stable.

---

## Interfaces
- `TensorEngine`: keeps existing public API signatures unchanged.
- `PhaseEncoder` (internal interface): provides phase-encoding behavior for engine runtime paths.
- Contract:
  - TensorEngine calls phase encoding through `PhaseEncoder` abstraction.
  - No direct phase-encoding logic remains in TensorEngine orchestration methods.

---

## Data Structures
- `PhaseEncodingInput`
  - Contains phase id, context metadata, and source tensor references.
  - Invariant: `phase_id` must be non-empty and valid for current runtime phase table.
- `PhaseEncodingOutput`
  - Contains encoded tensor payload and encoding metadata.
  - Invariant: output tensor shape must match TensorEngine downstream consumer expectations.

---

## State Transitions (if applicable)
- `uninitialized -> initialized -> encoding -> encoded -> dispatched`
- Invalid transitions are rejected and logged.

---

## Constraints
- No public API breaking changes in `TensorEngine`.
- Benchmark regression must remain under 2% for existing baseline scenarios.
- Memory overhead increase must remain under 5% for phase-encoding path.

---

## Invariants
- Public API signatures remain unchanged.
- Output tensor shape remains identical to v1.0.
- Numerical tolerance: max relative error <= 1e-6.
- No new cross-layer dependencies introduced.

---

## Behavioral Changes
- Internal phase encoding separated from amplitude logic.
- Computation order may change but results must be numerically equivalent within tolerance.

---

## Acceptance Criteria
- All existing TensorEngine tests pass.
- New/updated tests cover:
  - phase encoding delegation to `PhaseEncoder`
  - invalid transition rejection
  - output shape invariants
- Public API diff shows no breaking signature changes.
- Benchmark regression report confirms runtime regression < 2%.
