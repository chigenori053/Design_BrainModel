# Memory Engine Extraction Plan

## Current Status

Completed prerequisite reviews:

- `DBM_MEMORY_SPACE_PHASE14_DEPENDENCY_MIGRATION_PLAN_SPEC v1.0`
  - Status: `PLAN_ONLY`
  - Document: `docs/architecture/memory_space_phase14_dependency_migration_plan.md`
- `DBM_MEMORY_SPACE_PHASE14_STALE_LEGACY_DEPENDENCY_REVIEW_SPEC v1.0`
  - Status: `STALE_LEGACY_ISOLATED`
  - Document: `docs/architecture/memory_space_phase14_stale_legacy_dependency_review.md`

Current ownership:

- `memory_space_phase14`
  - path: `crates/memory_space_phase14`
  - status: `KEEP_EXPERIMENTAL`
  - current owner of `memory_space_phase14::stable_v03`
- Proposed extraction target:
  - path: `crates/memory_engine`
  - package: `memory_engine`
  - status: not yet created

This is a plan-only document. It does not create `crates/memory_engine`, move
APIs, change dependencies, change imports, delete code, or add deprecation
attributes.

## Source API Inventory

| Symbol | Current Path | Classification | Proposed Target | Action |
|---|---|---|---|---|
| `RecallInput` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with stable_v03 memory engine API; depends on `world_model::stable_v03::IntentState`. |
| `RecallConfig` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with current defaults and threshold/top-k behavior unchanged. |
| `MemoryQuery` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as the text/tags/limit retrieval query. |
| `MemoryRecord` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract unchanged; preserve `id`, `text`, `tags`, `embedding`, `architecture`, and `relations` fields. |
| `RecalledRecord` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with `MemoryRecord` and score semantics unchanged. |
| `RecallResult` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with `records` and `confidence` behavior unchanged. |
| `CacheStats` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with `InMemoryEngine` observability. |
| `MemoryNode` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as graph snapshot node type. |
| `MemoryRelation` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as graph snapshot edge relation type. |
| `MemoryEdge` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as graph snapshot edge type. |
| `MemoryGraphSnapshot` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract with `InMemoryEngine::graph_snapshot`. |
| `MemoryEngine` trait | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as the shared runtime memory engine trait with `recall`, `retrieve`, and `store`. |
| `InMemoryEngine` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Extract as deterministic in-memory backend with cache and graph snapshot behavior. |
| `InMemoryEngine::records` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Preserve return ordering and clone semantics. |
| `InMemoryEngine::recall_candidates` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Preserve `contracts::{MemoryCandidate, MemorySource}` mapping. |
| `InMemoryEngine::store_edge` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Preserve deterministic edge deduplication and ordering. |
| `InMemoryEngine::graph_snapshot` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Preserve snapshot node construction and ordering. |
| `InMemoryEngine::cache_stats` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Preserve hit/miss/eviction counters. |
| `contracts::{MemoryCandidate, MemoryId, MemorySource}` re-export | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Re-export or depend on `contracts` from the new crate to keep consumer type names stable. |
| `MemoryEngineError` | Not currently defined in `memory_space_phase14::stable_v03` | UNKNOWN | `memory_engine` only if introduced by a later spec | Do not invent in this plan; current API is infallible. |
| `MemoryEngineResult` | Not currently defined in `memory_space_phase14::stable_v03` | UNKNOWN | `memory_engine` only if introduced by a later spec | Do not invent in this plan; current API is infallible. |
| Private helpers: `normalized_terms`, `score_record`, `approximate_score`, `normalize_scores`, `prioritize_cluster_neighbors`, `Tap` | `memory_space_phase14::stable_v03` | MOVE_TO_MEMORY_ENGINE | `memory_engine` | Move or copy with `InMemoryEngine` implementation; keep private unless a later API spec promotes them. |
| `memory_space_core::MemoryEngine` and `memory_space_core::MemoryRecord` | `crates/core/memory_space_core` | DO_NOT_MOVE | `memory_space_core` | Keep separate; this utility API uses `MemoryStore`, `RecallQuery`, feature vectors, and numeric ids, not phase14 stable_v03 text/architecture records. |
| Pattern/search/architecture phase14 APIs | `memory_space_phase14::*` outside `stable_v03` | KEEP_PHASE14 | Later `search_memory` / `architecture_memory` specs | Do not move in this extraction plan. |

## Consumer Inventory

| Consumer | Current Import | Classification | Proposed Migration | Risk |
|---|---|---|---|---|
| `apps/cli/src/app.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | MIGRATE_DIRECT | Switch to `memory_engine::InMemoryEngine` after compatibility re-export exists. | Low; direct backend construction only. |
| `apps/cli/src/core.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | MIGRATE_DIRECT | Switch to `memory_engine::InMemoryEngine`. | Medium; CLI core has multiple runtime construction paths. |
| `apps/cli/src/dbm/client.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | MIGRATE_DIRECT | Switch to `memory_engine::InMemoryEngine`. | Low; direct backend construction. |
| `apps/cli/src/loop.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | MIGRATE_DIRECT | Switch to `memory_engine::InMemoryEngine`. | Low; direct intent refiner setup. |
| `apps/cli/src/memory_admin_main.rs` | `memory_space_phase14::stable_v03::MemoryRecord` | MIGRATE_DIRECT | Switch to `memory_engine::MemoryRecord`. | Medium; admin output/import behavior must preserve record shape. |
| `apps/cli/src/memory_seed.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord, MemoryQuery}` | MIGRATE_DIRECT | Switch to equivalent `memory_engine` imports. | Medium; seed/query behavior must remain deterministic. |
| `crates/runtime/runtime_core/src/intent_refiner/memory_adapter.rs` | `stable_v03::{MemoryEngine, MemoryQuery}` | MIGRATE_DIRECT | Switch adapter trait/query imports to `memory_engine`. | Medium; runtime adapter is production path. |
| `crates/runtime/runtime_core/src/intent_refiner/refiner.rs` | `stable_v03::MemoryEngine` | MIGRATE_DIRECT | Switch trait import to `memory_engine::MemoryEngine`. | Medium; trait object compatibility matters. |
| `crates/runtime/runtime_core/src/stable_v03.rs` | `stable_v03::{MemoryCandidate, MemoryEngine, MemoryRecord, MemorySource, RecallInput, RecallResult}` and explicit `RecalledRecord` / `RecallResult` paths | MIGRATE_AFTER_ADAPTER | Migrate after `memory_engine` exposes the full API and phase14 re-export compatibility is tested. | High; central runtime API and e2e tests use these types heavily. |
| `crates/runtime/runtime_core/tests/*.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | TEST_ONLY | Migrate after runtime production imports. | Medium; broad test coverage will catch compatibility drift. |
| `crates/memory_persistence/src/persistence_store.rs` | `stable_v03::MemoryRecord` | MIGRATE_DIRECT | Switch persistence to `memory_engine::MemoryRecord`. | High; persistence semantics and record format must remain unchanged. |
| `crates/knowledge_engine/src/lib.rs` | `memory_space_phase14::stable_v03::MemoryRecord` | MIGRATE_DIRECT | Switch record construction to `memory_engine::MemoryRecord`. | Medium; conversion output must be byte/field compatible. |
| `crates/knowledge_engine/tests/knowledge_core_integration.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, RecallInput}` | TEST_ONLY | Migrate after knowledge production conversion. | Low; test-only direct memory engine use. |
| `crates/code_language_core/src/stable_v03.rs` | `stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` | MIGRATE_DIRECT | Switch profile resolver memory imports to `memory_engine`. | High; language profile resolution depends on query/record behavior. |
| `crates/code_language_core/tests/*.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | TEST_ONLY | Migrate after code language production imports. | Medium; tests seed memory records and assert deterministic language behavior. |
| `crates/legacy/codegen_core_old/**` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord, MemoryQuery}` | TEST_ONLY | Do not migrate in this plan; stale legacy is isolated. | Medium if built directly; outside active workspace graph. |
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | phase14 self-test imports for `stable_v03` API | TEST_ONLY | Move or duplicate coverage into `memory_engine` tests during extraction, then keep compatibility tests in phase14. | Medium; this is the canonical behavior test set for extraction. |
| Other `stable_v03` matches in architecture/code/runtime crates | Non-memory `stable_v03` modules | BLOCKED | Not a memory engine migration target. | Low; broad search pattern is noisy and must be filtered by `memory_space_phase14::stable_v03`. |
| `memory_space_api::MemoryEngine` and `memory_space_core::MemoryEngine` consumers | Existing separate memory APIs | BLOCKED | Do not migrate as part of phase14 stable_v03 extraction. | Medium naming collision risk; imports must stay explicit. |

## Proposed Crate Boundary

Proposed crate:

- path: `crates/memory_engine`
- package: `memory_engine`

Responsibilities:

- Lightweight runtime memory engine trait and backend.
- `MemoryRecord` storage, retrieval, update-by-id replacement, and recall.
- Deterministic `InMemoryEngine` for tests and local runtime wiring.
- Cache stats and graph snapshot behavior currently attached to
  `InMemoryEngine`.
- Shared memory engine API for CLI, runtime, persistence, knowledge, and
  code-language layers.

Initial dependencies expected from current API shape:

- `architecture_ir` for `ArchitectureGraph` embedded in `MemoryRecord`.
- `world_model` for `IntentState` embedded in `RecallInput`.
- `contracts` for `MemoryCandidate`, `MemoryId`, and `MemorySource`.

Non-responsibilities:

- Canonical file-backed runtime memory store from `crates/memory_space`.
- `memory_space_core` feature-vector utility API.
- Graph/state/exploration memory.
- Search pattern memory: `PatternStore`, `DesignExperience`, `SearchPrior`,
  `PatternMatch`, `InMemoryMemorySpace`.
- Architecture template/evaluation memory: `DesignMemorySpace`,
  `TemplateMemoryDomain`, `ArchitectureRecord`, `EvaluationRecord`.
- Legacy crate repair for `crates/legacy/codegen_core_old`.

## API Compatibility Policy

- Initial extraction must preserve public type names for the stable_v03 memory
  engine API.
- Initial extraction must preserve field names, field types, and constructor
  expectations for public structs.
- Behavior must not change for recall scoring, approximate retrieval,
  deterministic ordering, cache invalidation, cache statistics, edge storage,
  graph snapshots, and record replacement by id.
- Serialization and persistence format must not change for downstream
  `MemoryRecord` users.
- Consumer migration may use temporary compatibility re-exports from
  `memory_space_phase14::stable_v03`.
- Compatibility re-exports should be deprecated only after active consumers have
  migrated to `memory_engine`.
- No `MemoryEngineError` or `MemoryEngineResult` should be introduced during
  the initial extraction unless a later spec explicitly changes the API from
  infallible to fallible.
- Naming collisions with `memory_space_core::MemoryEngine` and
  `memory_space_api::MemoryEngine` must be handled by explicit imports and
  crate-qualified references in migration specs.

## Migration Order

Phase 1: scaffold `crates/memory_engine`

- Create the package, workspace member, and minimal crate boundary.
- Do not change consumers in the scaffold step.

Phase 2: copy or move stable_v03 API

- Transfer `RecallInput`, `RecallConfig`, `MemoryQuery`, `MemoryRecord`,
  `RecalledRecord`, `RecallResult`, `CacheStats`, graph snapshot types,
  `MemoryEngine`, `InMemoryEngine`, and private helper behavior.
- Port the existing `memory_space_phase14/tests/stable_v03_core.rs` coverage.

Phase 3: compatibility re-export from `memory_space_phase14`

- Keep `memory_space_phase14::stable_v03::*` available as a temporary
  compatibility surface.
- Prefer re-exporting from `memory_engine` over maintaining two independent
  implementations.

Phase 4: migrate consumers

- Migrate direct CLI imports.
- Migrate `memory_persistence` and `knowledge_engine` `MemoryRecord` usage.
- Migrate `code_language_core` profile resolver usage.
- Migrate `runtime_core` intent/refiner imports.
- Migrate `runtime_core::stable_v03` last because it combines multiple stable
  runtime APIs and explicit phase14 type paths.
- Migrate tests with their owning production crate.

Phase 5: deprecate phase14 stable_v03 compatibility re-export

- Add deprecation only after active consumers use `memory_engine` directly.

Phase 6: remove compatibility layer

- Remove phase14 stable_v03 compatibility only after a residual audit confirms
  no active consumer imports remain.

This document executes none of these phases.

## Blockers

- `crates/memory_engine` does not exist yet.
- The current stable_v03 API is infallible; the requested `MemoryEngineError`
  and `MemoryEngineResult` symbols do not exist and require a separate API
  decision if they are desired.
- `MemoryRecord` depends on `architecture_ir::stable_v03::ArchitectureGraph`,
  so the new crate boundary must accept an architecture dependency or introduce
  an abstraction in a later spec.
- `RecallInput` depends on `world_model::stable_v03::IntentState`, so the new
  crate boundary must accept a world-model dependency or introduce an
  abstraction in a later spec.
- `runtime_core::stable_v03` uses memory engine types in a central runtime API
  and has explicit `memory_space_phase14::stable_v03` paths; migrate it after
  simpler direct consumers.
- There are same-name memory engine APIs in `memory_space_core` and
  `memory_space_api`; extraction specs must avoid accidental cross-crate
  substitutions.
- `crates/legacy/codegen_core_old` remains stale but isolated; do not use it as
  a required migration blocker for active workspace consumers.

## Non-Goals

- Do not create `crates/memory_engine`.
- Do not move `stable_v03`.
- Do not change `Cargo.toml`.
- Do not change consumer imports.
- Do not change `memory_space_phase14` public API.
- Do not add deprecated attributes.
- Do not delete or rewrite tests.
- Do not repair `crates/legacy/codegen_core_old`.
- Do not migrate `PatternStore`, `DesignExperience`, `SearchPrior`,
  `PatternMatch`, `DesignMemorySpace`, `TemplateMemoryDomain`,
  `ArchitectureRecord`, or `EvaluationRecord`.
- Do not change canonical `memory_space`, `memory_space_core`, or
  `memory_space_api` APIs.

## Risk Assessment

- Extraction is feasible because the target API is concentrated in
  `crates/memory_space_phase14/src/stable_v03.rs`.
- The consumer set is active and broad across CLI, runtime, persistence,
  knowledge, and code-language crates, so direct removal from phase14 would be
  high risk.
- Compatibility re-exports are necessary to avoid a flag-day migration.
- The highest compatibility risks are `MemoryRecord` persistence semantics,
  runtime trait object usage with `Arc<dyn MemoryEngine>`, deterministic recall
  ordering, and cache/graph snapshot behavior.
- The naming overlap with existing `memory_space_core::MemoryEngine` and
  `memory_space_api::MemoryEngine` can cause accidental wrong imports if later
  specs use unqualified names.

## Decision

Status: PLAN_ONLY

Rationale:

- `stable_v03` is a good candidate for a dedicated `memory_engine` crate, but
  the new crate boundary and compatibility re-export must be established before
  consumers move.
- The current API has dependencies on `architecture_ir`, `world_model`, and
  `contracts` that must be accepted or explicitly redesigned in a later spec.
- Active consumers should migrate gradually, with tests moved alongside their
  owning production crate.

## Next Spec

DBM_MEMORY_ENGINE_CRATE_SCAFFOLD_SPEC v1.0
