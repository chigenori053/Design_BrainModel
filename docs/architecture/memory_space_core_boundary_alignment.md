# Memory Space Core Boundary Alignment

## Current Crate Roles

| Crate | Path | Role | Decision |
|---|---|---|---|
| `memory_space` | `crates/memory_space` | Canonical runtime MemorySpace crate | Keep as the runtime boundary for `MemorySpace`, `MemoryStore`, `FileMemoryStore`, `MemoryEntry`, interference memory, graph/state/exploration memory. |
| `memory_space_core` | `crates/core/memory_space_core` | Low-level utility crate | Keep as utility boundary for identity, lineage, deduplication, recall DTOs, modality DTOs, primitive in-memory records, and memory primitives. |
| `memory_space_phase14` | `crates/memory_space_phase14` | Experimental memory crate | Keep isolated as phase14 experimental memory until a deprecation or graduation review. |
| `dhm` | `crates/dhm` | Canonical runtime consumer | Keep depending on `memory_space`; do not add `memory_space_core` or `memory_space_phase14` as standard dependencies. |
| `apps/cli` | `apps/cli` | Mixed memory consumer | Keep current dependencies for now; classify `memory_space_core` as utility use and `memory_space_phase14` as experimental use. |
| `design_reasoning::holographic_semantic_memory` | `crates/design_reasoning/src/holographic_semantic_memory.rs` | Reasoning-local semantic memory | Keep local to reasoning; do not treat as canonical runtime storage. |

## Dependency Inventory

| Consumer | Dependency | Classification | Action |
|---|---|---|---|
| Workspace root | `memory_space = { path = "crates/memory_space" }` | CANONICAL_RUNTIME | Keep. |
| Workspace root | `memory_space_core = { path = "crates/core/memory_space_core" }` | CORE_UTILITY | Keep. |
| Workspace root | `memory_space_phase14 = { path = "crates/memory_space_phase14" }` | EXPERIMENTAL_PHASE14 | Keep pending phase14 review. |
| `dhm` | `memory_space` | CANONICAL_RUNTIME | OK. |
| `agent_core`, `field_engine`, `hybrid_vm`, `shm`, `chm` | `memory_space` | CANONICAL_RUNTIME | OK; these use graph/state/id/runtime-facing types. |
| `memory_space_complex`, `memory_space_recall`, `memory_space_api`, `memory_space_index`, `concept_field`, `world_model`, runtime/search crates, CLI | `memory_space_core` and companion utility crates | CORE_UTILITY | OK; keep low-level utility use outside `memory_space`. |
| `apps/cli` | `memory_space_core` | CORE_UTILITY | OK for semantic identity/projection/persistence utilities. |
| `apps/cli` | `memory_space_phase14` | EXPERIMENTAL_PHASE14 | REVIEW in phase14 deprecation/replacement spec. |
| runtime, architecture, policy, persistence, knowledge, verification, code language crates | `memory_space_phase14` | EXPERIMENTAL_PHASE14 | REVIEW; broad active usage prevents removal in this phase. |
| `crates/legacy/codegen_core_old` | `memory_space_phase14 = { path = "../../memory_space" }` | UNKNOWN | REVIEW; direct path appears stale after directory rename and should be handled by a separate blocker/deprecation review. |

## Import Inventory

| File | Import | Classification | Action |
|---|---|---|---|
| `crates/dhm/src/lib.rs` | `use memory_space::{FileMemoryStore, InterferenceMode, MemoryInterferenceTelemetry, MemorySpace}` | OK_CANONICAL | Keep. |
| `crates/dhm/src/lib.rs` tests | `use memory_space::{FileMemoryStore, InterferenceMode, MemoryStore}` | OK_CANONICAL | Keep. |
| `crates/agent_core/**`, `crates/field_engine/**`, `crates/hybrid_vm/**`, `crates/shm/**`, `crates/chm/**` | `memory_space::{DesignState, DesignNode, StructuralGraph, Uuid, Value, InterferenceMode, MemoryInterferenceTelemetry}` | OK_CANONICAL | Keep as runtime/type consumers. |
| `crates/memory_space_complex/**`, `crates/memory_space_recall/**`, `crates/memory_space_api/**`, `crates/memory_space_index/**` | `memory_space_core::*` / `memory_space_complex::*` | OK_CORE_UTILITY | Keep as layered utility crates. |
| `crates/runtime/**`, `crates/engine/design_search_engine/**`, `crates/reasoning_agent/**`, `crates/search_controller/**` | `memory_space_core`, `memory_space_complex`, `memory_space_api`, `memory_space_index` imports | OK_CORE_UTILITY | Keep; these are recall/search utility boundaries. |
| `apps/cli/src/runtime/**`, `apps/cli/src/memory_admin_main.rs` | `memory_space_core::*` | OK_CORE_UTILITY | Keep for CLI utility functions. |
| `apps/cli/src/app.rs`, `apps/cli/src/core.rs`, `apps/cli/src/loop.rs`, `apps/cli/src/dbm/client.rs`, `apps/cli/src/memory_seed.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` and related types | OK_EXPERIMENTAL | REVIEW in phase14 deprecation/replacement spec. |
| `crates/runtime/**`, `crates/architecture_search/**`, `crates/architecture_evaluator/**`, `crates/policy_engine/**`, `crates/memory_persistence/**`, `crates/knowledge_engine/**`, `crates/code_language_core/**`, `crates/search_verification/**` | `memory_space_phase14::*` | OK_EXPERIMENTAL | REVIEW; broad phase14 usage remains intentionally unchanged. |
| `crates/design_reasoning/src/holographic_semantic_memory.rs` | local `HolographicMemoryStore` and semantic memory types | REASONING_LOCAL | Keep local; do not merge into `memory_space`. |

## Boundary Rules

- `memory_space` owns the canonical runtime `MemorySpace`, `MemoryStore`,
  `FileMemoryStore`, `MemoryEntry`, interference memory, and runtime
  graph/state/exploration memory.
- `memory_space` must not absorb phase14 experiment logic, CLI projection logic,
  or reasoning-specific semantic synthesis.
- `memory_space_core` owns low-level primitives, identity/lineage utilities,
  deduplication, recall DTOs, modality DTOs, and utility stores such as
  `InMemoryMemoryStore`.
- `memory_space_core` must not own `FileMemoryStore`, the canonical runtime
  `MemorySpace`, CLI state, or phase14 policy.
- `memory_space_phase14` remains experimental and must not be treated as the
  canonical runtime API.
- `dhm` must continue using `memory_space` as its memory dependency and must not
  depend directly on `memory_space_core` or `memory_space_phase14`.
- `apps/cli` is a mixed consumer for now: `memory_space_core` is allowed as
  utility use, while `memory_space_phase14` is explicitly experimental.
- `design_reasoning::holographic_semantic_memory` remains reasoning-local.

## Violations

- No forbidden legacy API or path references were found in `crates`, `apps`, or
  root `Cargo.toml` for:
  - `holographic_store`
  - `HolographicVectorStore`
  - `HolographicVectorStoreAdapter`
  - `LegacyMemoryStore`
  - `LegacyStoreAdapter`
  - `memory_space_legacy`
- `dhm` has no `memory_space_core` or `memory_space_phase14` dependency.
- `memory_space_core` does not contain `FileMemoryStore` or a runtime
  `MemorySpace` type.

## Deferred Items

- `apps/cli` still depends on `memory_space_phase14::stable_v03::InMemoryEngine`.
  This is allowed in the current mixed-consumer state but should be reviewed in
  `DBM_MEMORY_SPACE_PHASE14_DEPRECATION_REVIEW_SPEC v1.0`.
- Many runtime/search/architecture/policy/persistence crates still use
  `memory_space_phase14`. This confirms that phase14 cannot be removed or
  silently replaced in this phase.
- `crates/legacy/codegen_core_old/Cargo.toml` has a direct path dependency
  `memory_space_phase14 = { path = "../../memory_space" }`. Because this spec
  forbids dependency rewrites, it is recorded for follow-up rather than changed.
- `memory_space_core` exports a utility trait named `MemoryStore` and
  `InMemoryMemoryStore`. This is acceptable as a primitive utility boundary, but
  future docs should distinguish it clearly from `memory_space::MemoryStore`.

## Decision

Status: **PARTIALLY_ALIGNED**

Reason: legacy references are absent, `dhm` is aligned to canonical
`memory_space`, `memory_space_core` does not own the canonical runtime store, and
`memory_space_phase14` is isolated as experimental. The alignment is partial
because `apps/cli` and several runtime/search crates still actively depend on
`memory_space_phase14`, and one legacy crate has a stale direct phase14 path that
requires a separate follow-up.

## Next Spec

`DBM_MEMORY_SPACE_PHASE14_DEPRECATION_REVIEW_SPEC v1.0`
