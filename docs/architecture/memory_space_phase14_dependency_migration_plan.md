# Memory Space Phase14 Dependency Migration Plan

## Current Status

Previous review:

- Spec: `DBM_MEMORY_SPACE_PHASE14_DEPRECATION_REVIEW_SPEC v1.0`
- Result: `KEEP_EXPERIMENTAL`
- Document: `docs/architecture/memory_space_phase14_deprecation_review.md`

Current crate roles:

- `memory_space`
  - path: `crates/memory_space`
  - role: CANONICAL_RUNTIME
- `memory_space_core`
  - path: `crates/core/memory_space_core`
  - role: CORE_UTILITY
  - note: the migration spec refers to `crates/memory_space_core`, but the
    current workspace path is `crates/core/memory_space_core`.
- `memory_space_phase14`
  - path: `crates/memory_space_phase14`
  - role: EXPERIMENTAL

This is a plan-only document. It does not change Cargo dependencies, imports,
public APIs, storage formats, or deprecation attributes.

## Consumer Inventory

| Consumer | Current Phase14 Use | Classification | Proposed Target | Action |
|---|---|---|---|---|
| `apps/cli` | `stable_v03::InMemoryEngine`, `MemoryEngine`, `MemoryRecord`, `MemoryQuery` | EXTRACT_NEW_CRATE | `crates/memory_engine` | Keep phase14 dependency until a memory engine extraction plan defines compatibility shims and CLI migration order. |
| `crates/runtime/runtime_core` | `stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord, RecalledRecord, RecallResult}` plus search controller use of `{InMemoryMemorySpace, MemorySpace, SearchPrior, store_state_experience}` | EXTRACT_NEW_CRATE | `crates/memory_engine` and `crates/search_memory` | Split by API group; migrate intent/refiner stable_v03 paths after memory engine extraction, then migrate search controller after search memory extraction. |
| `crates/runtime/runtime_vm` | Adapter and tests use `DesignMemorySpace`, design intent/template/evaluation records, and embedding helpers | EXTRACT_NEW_CRATE | `crates/architecture_memory` | Keep until architecture memory extraction defines record ownership and adapter compatibility. |
| `crates/search_verification` | `DesignExperience`, `MemorySpace`, `PatternId`, `architecture_hash` | EXTRACT_NEW_CRATE | `crates/search_memory` | Keep until search memory owns pattern/experience/hash APIs and verification tests are migrated. |
| `crates/engine/design_search_engine` | `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience`, `DesignExperience` in tests | EXTRACT_NEW_CRATE | `crates/search_memory` | Keep; this is a primary search memory consumer and should migrate after API extraction stabilizes. |
| `crates/architecture_search` | `DesignMemorySpace`, `DesignIntentRecord`, `TemplateRecord`, `TopologyType`, `DependencyRuleRecord`, `TemplateMetadata` | EXTRACT_NEW_CRATE | `crates/architecture_memory` | Keep until template and architecture-domain records are extracted together. |
| `crates/architecture_evaluator` | Phase14 design memory records and `DesignMemorySpace` tests | EXTRACT_NEW_CRATE | `crates/architecture_memory` | Keep until evaluator memory-record ownership is moved to architecture memory. |
| `crates/policy_engine` | `DesignPattern`, `PatternId`, `DesignExperience` | EXTRACT_NEW_CRATE | `crates/search_memory` | Keep; policy currently consumes search/pattern memory types rather than canonical runtime memory. |
| `crates/memory_persistence` | `stable_v03::MemoryRecord` | EXTRACT_NEW_CRATE | `crates/memory_engine` | Keep; persistence record format must follow memory engine extraction. |
| `crates/knowledge_engine` | Produces and tests `stable_v03::MemoryRecord`, `InMemoryEngine`, `MemoryEngine`, `RecallInput` | EXTRACT_NEW_CRATE | `crates/memory_engine` | Keep; migrate after `MemoryRecord` ownership is established in memory engine. |
| `crates/code_language_core` | `stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` and tests with `InMemoryEngine` | EXTRACT_NEW_CRATE | `crates/memory_engine` | Keep; migrate after stable_v03 compatibility surface is extracted. |
| `crates/legacy/codegen_core_old` | Stale dependency declaration `memory_space_phase14 = { path = "../../memory_space" }` and stable_v03 imports | STALE_LEGACY | Separate stale legacy decision | Do not migrate in this plan; review deletion, isolation, or dependency repair in the next spec. |
| `crates/memory_space_phase14` self-tests | Phase14 self imports for experience, pattern memory, and stable_v03 | KEEP_PHASE14 | `crates/memory_space_phase14` until extraction begins | Keep tests in place until each extracted crate has equivalent coverage. |
| `crates/dhm` | No phase14 dependency; canonical runtime memory consumer | MIGRATE_TO_MEMORY_SPACE | `crates/memory_space` | No action in phase14 migration; keep canonical. |
| `crates/memory_space` | Canonical runtime memory implementation | MIGRATE_TO_MEMORY_SPACE | `crates/memory_space` | No action in phase14 migration; keep canonical. |
| `crates/core/memory_space_core` | Core utility implementation | MIGRATE_TO_MEMORY_SPACE_CORE | `crates/core/memory_space_core` | No action in phase14 migration; keep utility boundary. |

## API Group Migration Plan

| API Group | Current Owner | Proposed Owner | Decision |
|---|---|---|---|
| `stable_v03::MemoryEngine` | `memory_space_phase14` | `crates/memory_engine` | EXTRACT_NEW_CRATE; define engine trait ownership outside phase14 because it is recall/retrieve/store memory, not canonical objective-vector persistence. |
| `stable_v03::InMemoryEngine` | `memory_space_phase14` | `crates/memory_engine` | EXTRACT_NEW_CRATE; keep behavior compatible for CLI, runtime, knowledge, and code language consumers. |
| `stable_v03::MemoryRecord`, `MemoryQuery`, `RecallInput`, `RecallResult`, `RecalledRecord`, `RecallConfig` | `memory_space_phase14` | `crates/memory_engine` | EXTRACT_NEW_CRATE; record shape differs from canonical `MemoryEntry` and must migrate as a unit. |
| `stable_v03::MemoryGraphSnapshot`, `MemoryNode`, `MemoryEdge`, `MemoryRelation`, `CacheStats` | `memory_space_phase14` | `crates/memory_engine` | EXTRACT_NEW_CRATE; keep with engine observability and graph snapshot behavior. |
| `DesignExperience`, `ExperienceStore` | `memory_space_phase14` | `crates/search_memory` | EXTRACT_NEW_CRATE; search experience history is not canonical runtime memory. |
| `PatternId`, `DesignPattern`, `PatternStore`, phase14 `MemorySpace` trait, `InMemoryMemorySpace` | `memory_space_phase14` | `crates/search_memory` | EXTRACT_NEW_CRATE; these APIs model search-domain learned patterns. |
| `PatternMatch`, `match_patterns`, `extract_pattern`, `architecture_hash`, `layer_sequence_from_state` | `memory_space_phase14` | `crates/search_memory` | EXTRACT_NEW_CRATE; move deterministic search helpers with pattern memory. |
| `SearchPrior`, `store_state_experience` | `memory_space_phase14` | `crates/search_memory` | EXTRACT_NEW_CRATE; action weighting and world-state conversion are search-controller concerns. |
| `DesignMemorySpace` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; owns architecture/template/evaluation/trace memory aggregate. |
| `TemplateMemoryDomain`, `TemplateRecord`, `TemplateMetadata`, `DependencyRuleRecord`, `TopologyType` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; template memory is architecture-domain memory. |
| `ArchitectureMemoryDomain`, `ArchitectureRecord`, `ArchitectureMetadata`, `architecture_hash_string` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; architecture record storage belongs with architecture memory. |
| `EvaluationMemoryDomain`, `EvaluationRecord`, `EvaluationScores`, `EvaluationMetricsV2`, `EvaluationDiagnostics` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; evaluation memory should move with architecture records. |
| `ReasoningTraceMemoryDomain`, `ReasoningTrace`, `SearchStep` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; trace domain is coupled to architecture search/evaluation history. |
| Phase14 `MemoryGraph`, `MemoryIndex`, `MemoryType`, `MemoryMetadata`, `RelationType`, `DesignIntentRecord` | `memory_space_phase14` | `crates/architecture_memory` | EXTRACT_NEW_CRATE; typed graph/index are part of architecture memory, not canonical runtime memory. |
| Canonical `MemorySpace`, `MemoryStore`, `FileMemoryStore`, `MemoryEntry` | `memory_space` | `crates/memory_space` | KEEP_CANONICAL; not a phase14 migration target. |
| Core identity/dedup/utility APIs | `memory_space_core` | `crates/core/memory_space_core` | KEEP_CANONICAL; no phase14 consumer should be moved here without a focused utility-boundary spec. |

## Migration Order

Phase 1: stale legacy dependency review

- Spec: `DBM_MEMORY_SPACE_PHASE14_STALE_LEGACY_DEPENDENCY_REVIEW_SPEC v1.0`
- Scope: decide whether `crates/legacy/codegen_core_old` should be deleted,
  repaired, excluded, or isolated.
- Reason: its dependency name/path mismatch can confuse later automated
  migration and dependency inventory.

Phase 2: stable_v03 extraction plan

- Spec: `DBM_MEMORY_ENGINE_EXTRACTION_PLAN_SPEC v1.0`
- Proposed crate: `crates/memory_engine`
- Primary consumers: `apps/cli`, `runtime_core`, `memory_persistence`,
  `knowledge_engine`, `code_language_core`.
- Required decision: whether to preserve a `stable_v03` module path via
  compatibility re-exports during migration.

Phase 3: search/pattern memory extraction plan

- Spec: `DBM_SEARCH_MEMORY_EXTRACTION_PLAN_SPEC v1.0`
- Proposed crate: `crates/search_memory`
- Primary consumers: `design_search_engine`, `search_verification`,
  `policy_engine`, and `runtime_core` search controller.
- Required decision: how to split `InMemoryMemorySpace`, which currently joins
  experience/pattern memory with `DesignMemorySpace`.

Phase 4: architecture memory extraction plan

- Spec: `DBM_ARCHITECTURE_MEMORY_EXTRACTION_PLAN_SPEC v1.0`
- Proposed crate: `crates/architecture_memory`
- Primary consumers: `architecture_search`, `architecture_evaluator`,
  `runtime_vm`.
- Required decision: whether the existing workspace crate named
  `architecture_memory` is the target owner or whether a rename/merge plan is
  required before extraction.

Phase 5: phase14 residual audit

- Spec: `DBM_MEMORY_SPACE_PHASE14_RESIDUAL_AUDIT_SPEC v1.0`
- Scope: confirm that remaining phase14 API is either empty, compatibility-only,
  intentionally experimental, or ready for deprecation/removal.

## Blockers

- `crates/legacy/codegen_core_old` has a stale dependency declaration:
  `memory_space_phase14 = { path = "../../memory_space" }`. It is outside the
  root workspace members and must be reviewed before broad dependency rewriting.
- `runtime_core` uses two unrelated phase14 groups: stable_v03 memory engine
  APIs and search/pattern APIs. It cannot migrate in one mechanical step without
  mixing ownership boundaries.
- `InMemoryMemorySpace` combines `ExperienceStore`, `PatternStore`, and
  `DesignMemorySpace`, so search memory and architecture memory extraction need
  an explicit split strategy.
- `crates/architecture_memory` already exists in the workspace. The architecture
  memory extraction spec must decide whether to use that crate as the proposed
  owner or create a new transitional crate.
- No canonical `memory_space` mapping exists for `stable_v03`, pattern memory,
  `SearchPrior`, or `DesignMemorySpace`.

## Non-Goals

- Do not delete `memory_space_phase14`.
- Do not change `Cargo.toml`.
- Do not change consumer imports.
- Do not move `stable_v03`.
- Do not move `PatternStore`, `SearchPrior`, `DesignMemorySpace`, or related
  public APIs.
- Do not change public API signatures.
- Do not add deprecated attributes.
- Do not change storage formats.
- Do not migrate CLI, runtime, search, architecture, policy, persistence,
  knowledge, code language, or legacy consumers in this spec.

## Decision

Status: PLAN_ONLY

Rationale:

- `memory_space_phase14` currently combines several responsibilities in one
  crate: stable memory engine, search/pattern memory, architecture memory, and
  compatibility helpers.
- Immediate migration to canonical `memory_space` is not viable because
  canonical runtime memory has a different contract and storage model.
- The correct next step is to resolve stale legacy dependency status first, then
  extract the independent API groups through focused plans.

## Next Spec

DBM_MEMORY_SPACE_PHASE14_STALE_LEGACY_DEPENDENCY_REVIEW_SPEC v1.0
