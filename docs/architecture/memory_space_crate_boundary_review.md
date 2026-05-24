# Memory Space Crate Boundary Review

## Current Crates

- `memory_space`
  - Package name: `memory_space`
  - Directory: `crates/memory_space`
  - Current public API includes `MemorySpace<S: MemoryStore = FileMemoryStore>`,
    `MemoryStore`, `FileMemoryStore`, `MemoryEntry`, `DesignState`, and graph
    model types.
  - Current direct consumer confirmed in this review: `dhm`.

- `memory_space_core`
  - Package name: `memory_space_core`
  - Directory: `crates/core/memory_space_core`
  - Provides low-level memory fields, recall DTOs, modality buffers,
    `MemoryEngine`, `MemoryStore`, `InMemoryMemoryStore`, and semantic identity
    / deduplication utilities.
  - Used broadly by recall, runtime, search, CLI, world model, and reasoning
    crates.

- `memory_space_phase14`
  - Package name: `memory_space_phase14`
  - Directory: `crates/memory_space_phase14`
  - Provides design experience, pattern memory, `DesignMemorySpace`,
    `InMemoryMemorySpace`, `stable_v03::MemoryEngine`, and search prior support.
  - Used by runtime, CLI, architecture search/evaluation, policy, persistence,
    knowledge, verification, and legacy codegen paths.

- `design_reasoning` memory modules
  - `holographic_semantic_memory` exposes `HolographicMemoryStore` and semantic
    memory governance types through the `design_reasoning` root.
  - The module is reasoning-domain local and should not be treated as the
    canonical runtime `MemorySpace` crate boundary.

- `dhm` dependency
  - `dhm` depends on the `memory_space` package and uses the canonical
    `FileMemoryStore`, `MemoryStore`, `MemorySpace`, and interference telemetry
    API.

- `DBM_CLI` dependency
  - `apps/cli` currently depends on `memory_space_phase14` and
    `memory_space_core`.
  - It uses `memory_space_phase14::stable_v03::InMemoryEngine` for CLI memory
    flows and `memory_space_core` for semantic identity, projection, and
    persistence utilities.
  - It does not currently depend on the `memory_space` package.

## Classification

| Component | Current Role | Classification | Keep / Rename / Merge / Deprecate | Reason |
|---|---|---|---|---|
| `memory_space` package | File-backed interference MemorySpace and canonical store API for dhm/runtime-style recall-first use | CANONICAL_RUNTIME | KEEP | It now has canonical public names and `dhm` uses it through `FileMemoryStore`; package name is already `memory_space`. |
| `crates/memory_space` directory | Directory containing the `memory_space` package | CANONICAL_RUNTIME | KEEP | The directory now matches the canonical package name. |
| `memory_space::MemoryStore` | Store trait for `MemoryEntry` persistence | CANONICAL_RUNTIME | KEEP | This is the canonical store trait for the `memory_space` package after alias removal. |
| `memory_space::FileMemoryStore` | File-backed `MemoryEntry` store preserving the existing binary format | CANONICAL_RUNTIME | KEEP | This is the canonical store implementation used by `dhm`; storage format remains unchanged. |
| `memory_space::MemorySpace<S>` | Interference memory runtime with generic store boundary | CANONICAL_RUNTIME | KEEP | This is the canonical runtime MemorySpace for the current `memory_space` package. |
| `memory_space_core` | Low-level recall DTOs, complex fields, modality DTOs, in-memory store, semantic identity, deduplication | CORE_UTILITY | KEEP | It is broadly reused as utility infrastructure and is not the file-backed runtime store boundary. |
| `memory_space_complex` | Complex vector algebra and encoding over `memory_space_core::Complex64` | CORE_UTILITY | KEEP | Supports recall/search math rather than runtime persistence. |
| `memory_space_recall` | Recall scoring/search utilities over core fields | CORE_UTILITY | KEEP | Layered utility used by memory API and indexing. |
| `memory_space_eval` | Recall evaluation/scoring utilities | CORE_UTILITY | KEEP | Support crate for recall quality evaluation. |
| `memory_space_index` | Index abstraction and linear search over complex fields | CORE_UTILITY | KEEP | Indexing layer for recall systems. |
| `memory_space_api` | Concept recall API and `MemoryEngine` facade over complex/core/index crates | CORE_UTILITY | KEEP | Runtime/search components use it as a concept recall facade, not as the canonical file-backed `memory_space` package. |
| `memory_space_phase14` package | Design experience, pattern memory, `DesignMemorySpace`, `InMemoryMemorySpace`, and `stable_v03` memory engine | EXPERIMENTAL | REVIEW | It has many active consumers, including CLI/runtime, but its package name and directory are phase-scoped and separate from `memory_space`. |
| `memory_space_phase14::stable_v03` | In-memory engine used by CLI, runtime core, knowledge, code language, and legacy paths | EXPERIMENTAL | REVIEW | Widely used and stable-looking, but still housed under phase14; boundary review should decide whether it remains phase-local or graduates. |
| `design_reasoning::holographic_semantic_memory` | Reasoning-domain semantic memory and governance store | REASONING_LOCAL | KEEP_LOCAL | It is exported by `design_reasoning` and tied to semantic reasoning concepts, not the canonical runtime store API. |
| `dhm` | Consumer of `memory_space` interference memory | CONSUMER | KEEP | It should continue depending on the canonical `memory_space` package API. |
| `apps/cli` | Consumer of `memory_space_phase14::stable_v03` and `memory_space_core` utilities | CONSUMER | REVIEW | CLI does not currently consume `memory_space`; decide separately whether CLI should continue using phase14/stable_v03 or move to a future unified boundary. |

## Canonical Decision

- Canonical runtime MemorySpace:
  - `memory_space::MemorySpace<S: MemoryStore = FileMemoryStore>` from the
    `memory_space` package in `crates/memory_space`.

- Canonical store API:
  - `memory_space::MemoryStore`
  - `memory_space::FileMemoryStore`
  - `memory_space::MemoryEntry`

- Core utility memory:
  - `memory_space_core`
  - `memory_space_complex`
  - `memory_space_recall`
  - `memory_space_eval`
  - `memory_space_index`
  - `memory_space_api`

- Experimental memory:
  - `memory_space_phase14`
  - `memory_space_phase14::stable_v03`
  - `memory_space_phase14::DesignMemorySpace`
  - `memory_space_phase14::InMemoryMemorySpace`

- Reasoning-local memory:
  - `design_reasoning::holographic_semantic_memory`
  - `design_reasoning::HolographicMemoryStore`

- Deprecated memory:
  - No deprecated `memory_space` public aliases remain.
  - No `holographic_store` module remains.

## Dependency Findings

- Workspace dependency aliases map `memory_space` to
  `crates/memory_space` and `memory_space_phase14` to
  `crates/memory_space_phase14`.
- `dhm` depends on `memory_space` and uses the canonical file store boundary.
- `apps/cli` depends on `memory_space_phase14` and `memory_space_core`, not the
  `memory_space` package.
- Runtime/search stacks use a mix of `memory_space_core`,
  `memory_space_complex`, `memory_space_api`, and `memory_space_phase14`.
- `memory_space_phase14` remains heavily consumed and should not be deleted or
  merged without a separate deprecation or graduation review.

## Non-actions

- No rename in this phase.
- No crate merge in this phase.
- No public API break in this phase.
- No package name change in this phase.
- No directory rename in this phase.
- No storage format change in this phase.
- No migration of `memory_space_phase14`, `memory_space_core`, or
  `design_reasoning` memory modules in this phase.

## Next Recommended Spec

Recommended next step:

`DBM_MEMORY_SPACE_CORE_BOUNDARY_ALIGNMENT_SPEC v1.0`

Reason: the canonical runtime package and directory now both use
`memory_space`. The next remaining boundary question is how `memory_space`,
`memory_space_core`, and `memory_space_phase14` should be documented and consumed
by CLI, dhm, and reasoning systems.

Secondary follow-up candidates:

- `DBM_MEMORY_SPACE_CORE_BOUNDARY_ALIGNMENT_SPEC v1.0`
- `DBM_MEMORY_SPACE_PHASE14_DEPRECATION_REVIEW_SPEC v1.0`
- `DBM_DESIGN_REASONING_MEMORY_BOUNDARY_SPEC v1.0`
