# Memory Space Phase14 Deprecation Review

## Current Role

- `memory_space_phase14`
  - package: `memory_space_phase14`
  - path: `crates/memory_space_phase14`
  - role: EXPERIMENTAL

- `memory_space`
  - package: `memory_space`
  - path: `crates/memory_space`
  - role: CANONICAL_RUNTIME

Structure check:

- `crates/memory_space` exists.
- `crates/memory_space_phase14` exists.
- `crates/memory_space_legacy` is absent.
- Workspace dependency for canonical runtime memory is
  `memory_space = { path = "crates/memory_space" }`.
- Workspace dependency for phase14 memory is
  `memory_space_phase14 = { path = "crates/memory_space_phase14" }`.

## Cargo Dependency Inventory

| Consumer | Path | Classification | Action |
|---|---|---|---|
| Workspace member list | `Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep `crates/memory_space_phase14` as a workspace member during this review. |
| Workspace dependency alias | `Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep `memory_space_phase14 = { path = "crates/memory_space_phase14" }`; no dependency change in this spec. |
| `apps/cli` | `apps/cli/Cargo.toml` | ACTIVE_CLI_DEPENDENCY | Keep; CLI uses `stable_v03::InMemoryEngine` and `MemoryRecord`. |
| `runtime_core` | `crates/runtime/runtime_core/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; runtime intent refinement and stable memory paths use phase14. |
| `runtime_vm` | `crates/runtime/runtime_vm/Cargo.toml` | TEST_ONLY_DEPENDENCY | Keep until VM memory integration tests have a canonical replacement. |
| `search_verification` | `crates/search_verification/Cargo.toml` | ACTIVE_SEARCH_DEPENDENCY | Keep; library and tests use phase14 pattern and hash APIs. |
| `design_search_engine` | `crates/engine/design_search_engine/Cargo.toml` | ACTIVE_SEARCH_DEPENDENCY | Keep; beam search controller uses phase14 pattern memory and search prior APIs. |
| `architecture_search` | `crates/architecture_search/Cargo.toml` | ACTIVE_SEARCH_DEPENDENCY | Keep; search engine and template engine use `DesignMemorySpace` records. |
| `architecture_evaluator` | `crates/architecture_evaluator/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; evaluator uses phase14 design memory records. |
| `policy_engine` | `crates/policy_engine/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; policy model, store, evaluator, and generalizer use phase14 pattern/experience types. |
| `memory_persistence` | `crates/memory_persistence/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; persistence store uses `stable_v03::MemoryRecord`. |
| `knowledge_engine` | `crates/knowledge_engine/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; conversion code and integration tests use `stable_v03` records/engine. |
| `code_language_core` | `crates/code_language_core/Cargo.toml` | ACTIVE_RUNTIME_DEPENDENCY | Keep; stable_v03 adapter and tests use phase14 memory engine contracts. |
| `crates/legacy/codegen_core_old` | `crates/legacy/codegen_core_old/Cargo.toml` | STALE_LEGACY_DEPENDENCY | Review separately; it declares `memory_space_phase14 = { path = "../../memory_space" }`, which points at the canonical crate path while using the phase14 dependency name. This crate is not listed as a workspace member. |
| `memory_space_phase14` package metadata | `crates/memory_space_phase14/Cargo.toml` | UNKNOWN | Not a consumer dependency; record only as package identity. |

## Rust Import Inventory

| File | Import | Classification | Action |
|---|---|---|---|
| `apps/cli/src/app.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | ACTIVE_CLI_DEPENDENCY | Keep until CLI memory engine migration plan exists. |
| `apps/cli/src/core.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | ACTIVE_CLI_DEPENDENCY | Keep until CLI memory engine migration plan exists. |
| `apps/cli/src/dbm/client.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | ACTIVE_CLI_DEPENDENCY | Keep until CLI memory engine migration plan exists. |
| `apps/cli/src/loop.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | ACTIVE_CLI_DEPENDENCY | Keep until CLI memory engine migration plan exists. |
| `apps/cli/src/memory_admin_main.rs` | `memory_space_phase14::stable_v03::MemoryRecord` | ACTIVE_CLI_DEPENDENCY | Keep; admin memory record format has no canonical replacement in this review. |
| `apps/cli/src/memory_seed.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord, MemoryQuery}` | ACTIVE_CLI_DEPENDENCY | Keep; seed and query flows depend on stable_v03. |
| `crates/runtime/runtime_core/src/intent_refiner/memory_adapter.rs` | `stable_v03::{MemoryEngine, MemoryQuery}` | ACTIVE_RUNTIME_DEPENDENCY | Keep; active runtime adapter. |
| `crates/runtime/runtime_core/src/intent_refiner/refiner.rs` | `stable_v03::MemoryEngine` | ACTIVE_RUNTIME_DEPENDENCY | Keep; active runtime refiner. |
| `crates/runtime/runtime_core/src/search/beam_search_controller.rs` | `{InMemoryMemorySpace, MemorySpace, SearchPrior, store_state_experience}` | ACTIVE_SEARCH_DEPENDENCY | Keep; search controller relies on phase14 pattern memory contract. |
| `crates/runtime/runtime_core/src/stable_v03.rs` | `stable_v03::{...}` and `stable_v03::RecalledRecord` | ACTIVE_RUNTIME_DEPENDENCY | Keep; stable runtime memory API. |
| `crates/runtime/runtime_core/tests/*.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | TEST_ONLY_DEPENDENCY | Keep until runtime tests are migrated with production code. |
| `crates/runtime/runtime_vm/src/adapter.rs` | phase14 design memory records and embedding helpers | ACTIVE_RUNTIME_DEPENDENCY | Keep; VM adapter has active phase14 data model coupling. |
| `crates/runtime/runtime_vm/tests/*.rs` | phase14 design memory records and `DesignMemorySpace` | TEST_ONLY_DEPENDENCY | Keep until VM memory integration surface changes. |
| `crates/search_verification/src/lib.rs` | `{DesignExperience, MemorySpace, PatternId, architecture_hash}` | ACTIVE_SEARCH_DEPENDENCY | Keep; verification library uses pattern memory APIs. |
| `crates/search_verification/tests/*.rs` | `architecture_hash`, `DesignExperience`, `MemorySpace` | TEST_ONLY_DEPENDENCY | Keep until verification library is migrated. |
| `crates/engine/design_search_engine/src/beam_search_controller.rs` | `{InMemoryMemorySpace, MemorySpace, SearchPrior, store_state_experience}` | ACTIVE_SEARCH_DEPENDENCY | Keep; active search engine consumer. |
| `crates/engine/design_search_engine/tests/experiments/experience_prior.rs` | `{DesignExperience, MemorySpace}` | TEST_ONLY_DEPENDENCY | Keep with search engine migration. |
| `crates/architecture_search/src/engine.rs` | phase14 design memory types | ACTIVE_SEARCH_DEPENDENCY | Keep; architecture search depends on `DesignMemorySpace`. |
| `crates/architecture_search/src/template_engine.rs` | `DesignIntentRecord`, `DesignMemorySpace`, `TemplateRecord`, `TopologyType`, `DependencyRuleRecord`, `TemplateMetadata` | ACTIVE_SEARCH_DEPENDENCY | Keep; template memory has no 1:1 canonical equivalent. |
| `crates/architecture_search/tests/core.rs` | `DesignMemorySpace` | TEST_ONLY_DEPENDENCY | Keep until architecture search migration. |
| `crates/architecture_evaluator/src/lib.rs` | phase14 design memory types | ACTIVE_RUNTIME_DEPENDENCY | Keep; evaluator uses phase14 memory records. |
| `crates/architecture_evaluator/tests/ir_evaluation.rs` | `DesignMemorySpace` | TEST_ONLY_DEPENDENCY | Keep until evaluator migration. |
| `crates/policy_engine/src/*.rs` | `DesignPattern`, `PatternId`, `DesignExperience` | ACTIVE_RUNTIME_DEPENDENCY | Keep; policy engine uses pattern and experience contracts. |
| `crates/policy_engine/tests/*.rs` | `DesignExperience`, `DesignPattern`, `PatternId` | TEST_ONLY_DEPENDENCY | Keep with policy engine migration. |
| `crates/memory_persistence/src/persistence_store.rs` | `stable_v03::MemoryRecord` | ACTIVE_RUNTIME_DEPENDENCY | Keep; persistence record format remains phase14-owned. |
| `crates/knowledge_engine/src/lib.rs` | `stable_v03::MemoryRecord` | ACTIVE_RUNTIME_DEPENDENCY | Keep; conversion emits phase14 stable records. |
| `crates/knowledge_engine/tests/knowledge_core_integration.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, RecallInput}` | TEST_ONLY_DEPENDENCY | Keep until knowledge memory migration. |
| `crates/code_language_core/src/stable_v03.rs` | `stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` | ACTIVE_RUNTIME_DEPENDENCY | Keep; code language stable adapter uses phase14 memory engine. |
| `crates/code_language_core/tests/*.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | TEST_ONLY_DEPENDENCY | Keep until adapter migration. |
| `crates/legacy/codegen_core_old/src/stable_v03.rs` | `stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` | STALE_USE | Review with stale legacy dependency; crate is outside workspace membership. |
| `crates/legacy/codegen_core_old/tests/*.rs` | `stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | STALE_USE | Review with stale legacy dependency. |
| `crates/memory_space_phase14/tests/*.rs` | phase14 crate self imports | TEST_ONLY_DEPENDENCY | Keep as self-tests for the experimental crate. |

## API Surface Inventory

| Symbol | Classification | Action |
|---|---|---|
| `DesignExperience` | EXPERIMENTAL_ONLY | Keep; captures semantic context, inferred semantics, design-domain architecture, causal graph, dependency edges, layer sequence, score, and search depth. |
| `ExperienceStore` | EXPERIMENTAL_ONLY | Keep; no 1:1 canonical store equivalent because canonical `MemoryStore` persists objective-vector `MemoryEntry` values. |
| `PatternId` | EXPERIMENTAL_ONLY | Keep; used by policy and search consumers. |
| `DesignPattern` | EXPERIMENTAL_ONLY | Keep; represents causal/layer/frequency/score patterns not present in canonical `memory_space`. |
| `MemorySpace` trait in `pattern_store.rs` | EXPERIMENTAL_ONLY | Keep; this is a pattern recall/store-experience trait, not the canonical `MemorySpace` struct. |
| `PatternStore` | EXPERIMENTAL_ONLY | Keep; pattern indexing and upsert behavior has no canonical equivalent. |
| `InMemoryMemorySpace` | EXPERIMENTAL_ONLY | Keep; combines `ExperienceStore`, `PatternStore`, and `DesignMemorySpace`. |
| `signature_for_pattern` | EXPERIMENTAL_ONLY | Keep; helper for phase14 pattern identity. |
| `store_state_experience` | SEARCH_ONLY | Keep; used by search controllers to convert `WorldState` into phase14 experience. |
| `PatternMatch` | SEARCH_ONLY | Keep; search pattern matching result. |
| `match_patterns` | SEARCH_ONLY | Keep; no canonical equivalent. |
| `SearchPrior` | SEARCH_ONLY | Keep; search action weighting still depends on phase14 patterns. |
| `architecture_hash` | SEARCH_ONLY | Keep; active verification/search tests use deterministic state hash. |
| `extract_pattern` | SEARCH_ONLY | Keep; converts experiences to patterns. |
| `layer_sequence_from_state` | SEARCH_ONLY | Keep; search-state helper. |
| `DesignMemorySpace` | EXPERIMENTAL_ONLY | Keep; graph/index/domain memory aggregate is distinct from canonical runtime memory. |
| `TemplateLearningEvent` | EXPERIMENTAL_ONLY | Keep; tied to phase14 template learning. |
| `embed_intent`, `embed_template`, `embed_architecture`, `embed_evaluation` | EXPERIMENTAL_ONLY | Keep; phase14 embedding helpers used by architecture/search/runtime consumers. |
| `TopologyType`, `DependencyRuleRecord`, `TemplateMetadata`, `TemplateRecord`, `TemplateMemoryDomain` | EXPERIMENTAL_ONLY | Keep; template memory domain has no canonical runtime counterpart. |
| `ArchitectureMetadata`, `ArchitectureRecord`, `ArchitectureMemoryDomain`, `architecture_hash_string` | EXPERIMENTAL_ONLY | Keep; architecture-domain memory records differ from canonical graph/state types. |
| `EvaluationScores`, `EvaluationMetricsV2`, `EvaluationDiagnostics`, `EvaluationRecord`, `EvaluationMemoryDomain` | EXPERIMENTAL_ONLY | Keep; evaluation memory is phase14-specific. |
| `ReasoningTrace`, `ReasoningTraceMemoryDomain`, `SearchStep` | EXPERIMENTAL_ONLY | Keep; trace domain has no canonical equivalent. |
| `MemoryGraph`, `MemoryIndex` under `MemorySpace` module | EXPERIMENTAL_ONLY | Keep; graph/index are internal to phase14 design memory. |
| `DesignIntentRecord`, phase14 `MemoryNode`, `MemoryEdge`, `MemoryType`, `MemoryMetadata`, `RelationType` | EXPERIMENTAL_ONLY | Keep; record graph model differs from canonical `StructuralGraph`. |
| `stable_v03::MemoryEngine` | EXPERIMENTAL_ONLY | Keep; active runtime/CLI/code-language API. |
| `stable_v03::InMemoryEngine` | CLI_ONLY | Keep; active CLI and tests use it directly. |
| `stable_v03::RecallInput`, `RecallConfig`, `MemoryQuery`, `MemoryRecord`, `RecalledRecord`, `RecallResult`, `CacheStats`, `MemoryGraphSnapshot`, `MemoryRelation`, `MemoryNode`, `MemoryEdge` | EXPERIMENTAL_ONLY | Keep; stable_v03 consumer surface has no 1:1 canonical mapping. |
| `stable_v03` re-export of `contracts::{MemoryCandidate, MemoryId, MemorySource}` | CANONICAL_CANDIDATE | Keep for now; contracts may belong outside phase14, but migration requires a separate plan. |

## Canonical Overlap

| Phase14 Capability | Canonical Equivalent | Decision |
|---|---|---|
| `pattern_store::MemorySpace` trait | Canonical `memory_space::MemorySpace` struct | KEEP_EXPERIMENTAL; names overlap, but contracts differ. Phase14 recalls patterns and stores `DesignExperience`; canonical applies/stores objective-vector memory interference via a `MemoryStore`. |
| `InMemoryMemorySpace` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical runtime memory has `MemorySpace<S>` over `MemoryStore`, not an in-memory pattern/template/experience aggregate. |
| `DesignExperience` / `ExperienceStore` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical `MemoryEntry` stores id/depth/timestamp/vector only. |
| `PatternStore` / `PatternMatch` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical graph/state modules model design nodes and DAG edges, not learned causal/layer patterns. |
| `SearchPrior` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical runtime memory does not weight search actions. |
| `DesignMemorySpace` graph/index/domain aggregate | Partial overlap with canonical `StructuralGraph`, `DesignState`, and `ExplorationMemory` | KEEP_EXPERIMENTAL; canonical graph/state APIs represent runtime structure, while phase14 stores templates, architectures, evaluations, traces, embeddings, and relation propagation. |
| `stable_v03::MemoryEngine` and `InMemoryEngine` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical `MemoryStore` is persistence for objective-vector entries, not a recall/retrieve/store text-and-architecture memory engine. |
| `stable_v03::MemoryRecord` | Canonical `MemoryEntry` | KEEP_EXPERIMENTAL; both are memory records, but fields and storage semantics differ. |
| `MemorySpace::store_architecture`, `store_evaluation`, `store_reasoning_trace` | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical memory has no architecture/evaluation/trace domains. |
| `embed_*` helpers | No 1:1 equivalent | KEEP_EXPERIMENTAL; canonical runtime memory consumes objective vectors and does not expose these embedding functions. |
| Phase14 `MemoryGraph` and `MemoryIndex` | Canonical `StructuralGraph` | KEEP_EXPERIMENTAL; both are graph-like, but phase14 graph supports typed memory nodes, embeddings, relation weights, nearest search, and activation propagation. |

## Stale Dependencies

- No `crates/memory_space_legacy` directory was found.
- The workspace canonical dependency is correct:
  `memory_space = { path = "crates/memory_space" }`.
- The workspace phase14 dependency is correct:
  `memory_space_phase14 = { path = "crates/memory_space_phase14" }`.
- `rg` for the requested forbidden stale path expression reported only the
  workspace phase14 dependency.
- Additional inventory found one stale legacy dependency outside the workspace:
  `crates/legacy/codegen_core_old/Cargo.toml` declares
  `memory_space_phase14 = { path = "../../memory_space" }`. This path resolves to
  the canonical crate directory while the dependency name remains phase14. The
  crate is not listed in the root workspace members, so this review records it
  as `STALE_LEGACY_DEPENDENCY` and does not modify it.

## Risk Assessment

- `memory_space_phase14` has active consumers across CLI, runtime, search,
  policy, persistence, knowledge, architecture, and code language crates.
- The phase14 API surface is not a thin wrapper around canonical `memory_space`.
  It includes pattern memory, template memory, architecture/evaluation/trace
  domains, a stable_v03 engine, and search-prior behavior.
- Immediate deprecation or removal would risk breaking CLI memory flows,
  runtime intent refinement, search controller behavior, policy tests, memory
  persistence, and architecture/search integration tests.
- Canonical `memory_space` is narrower and runtime-focused: it stores and
  applies interference from objective-vector `MemoryEntry` values via
  `MemoryStore`/`FileMemoryStore`, plus graph/state/exploration types.
- The stale legacy path is a migration blocker for any future repo-wide rename
  or dependency cleanup, but it is outside workspace membership and should be
  handled by a separate blocker or migration spec.

## Decision

Status: KEEP_EXPERIMENTAL

Rationale:

- Active dependency/imports exist in runtime, search, CLI, policy, persistence,
  knowledge, architecture, and code language consumers.
- Canonical `memory_space` does not provide 1:1 replacements for phase14
  pattern memory, `stable_v03::MemoryEngine`, `DesignMemorySpace`, or search
  prior APIs.
- Search/runtime/CLI paths still depend on phase14 behavior.
- Deprecation, migration, or deletion would require a consumer-by-consumer
  migration plan and API mapping that are outside this review.

## Next Spec

DBM_MEMORY_SPACE_PHASE14_DEPENDENCY_MIGRATION_PLAN_SPEC v1.0
