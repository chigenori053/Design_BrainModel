# Memory Space Phase14 Search API Migration Review

## Current Status

Status: SEARCH_API_EXTRACTION_REQUIRED

`memory_engine` now owns the runtime memory engine API. `memory_space_phase14` no longer exposes `stable_v03`; its remaining active role is a mixed experimental API surface for search, pattern, experience, and architecture memory.

The remaining phase14 APIs are active across multiple consumers, so `memory_space_phase14` is not a delete candidate yet.

## Cargo Dependency Inventory

| Consumer | Path | Classification | Action |
|---|---|---|---|
| Workspace | `Cargo.toml` members and workspace dependency | WORKSPACE_DEPENDENCY | Keep until phase14 residual APIs are extracted. |
| `memory_space_phase14` | `crates/memory_space_phase14/Cargo.toml` package definition | WORKSPACE_DEPENDENCY | Keep; source owner for remaining APIs. |
| `runtime_core` | `crates/runtime/runtime_core/Cargo.toml` | ACTIVE_SEARCH_API_CONSUMER | Keep; uses `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience`. |
| `design_search_engine` | `crates/engine/design_search_engine/Cargo.toml` | ACTIVE_SEARCH_API_CONSUMER | Keep; uses beam-search memory and search prior APIs. |
| `policy_engine` | `crates/policy_engine/Cargo.toml` | ACTIVE_PATTERN_API_CONSUMER | Keep; uses `DesignPattern`, `PatternId`, and `DesignExperience`. |
| `search_verification` | `crates/search_verification/Cargo.toml` | ACTIVE_EXPERIENCE_API_CONSUMER | Keep; uses `DesignExperience`, `MemorySpace`, `PatternId`, `architecture_hash`. |
| `architecture_search` | `crates/architecture_search/Cargo.toml` | ACTIVE_SEARCH_API_CONSUMER | Keep; uses architecture memory and reasoning trace APIs. |
| `architecture_evaluator` | `crates/architecture_evaluator/Cargo.toml` | ACTIVE_EXPERIENCE_API_CONSUMER | Keep; uses `DesignMemorySpace`, evaluation record types, and `embed_evaluation`. |
| `runtime_vm` | `crates/runtime/runtime_vm/Cargo.toml` | ACTIVE_EXPERIENCE_API_CONSUMER | Keep; uses design experience and architecture memory APIs. |
| `crates/legacy/codegen_core_old` | `crates/legacy/codegen_core_old/Cargo.toml` | LEGACY_ISOLATED | Out of scope; stale path and removed `stable_v03` refs remain legacy cleanup work. |

## Rust Import Inventory

| File | Import | Classification | Action |
|---|---|---|---|
| `crates/runtime/runtime_core/src/search/beam_search_controller.rs` | `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience` | SEARCH_API_USE | Migrate with search memory extraction. |
| `crates/engine/design_search_engine/src/beam_search_controller.rs` | `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience` | SEARCH_API_USE | Migrate with search memory extraction; this is the likely anchor consumer. |
| `crates/policy_engine/src/policy_evaluator.rs` | `DesignPattern` | PATTERN_API_USE | Move with pattern model APIs. |
| `crates/policy_engine/src/policy_model.rs` | `PatternId` | PATTERN_API_USE | Move with pattern model APIs. |
| `crates/policy_engine/src/pattern_generalizer.rs` | `DesignPattern`, `PatternId` | PATTERN_API_USE | Move with pattern extraction/generalization APIs. |
| `crates/policy_engine/src/policy_store.rs` | `DesignExperience` | EXPERIENCE_API_USE | Move with experience model or search memory APIs. |
| `crates/policy_engine/tests/*.rs` | `DesignExperience`, `DesignPattern`, `PatternId` | TEST_ONLY_USE | Migrate after production policy imports. |
| `crates/search_verification/src/lib.rs` | `DesignExperience`, `MemorySpace`, `PatternId`, `architecture_hash` | EXPERIENCE_API_USE | Migrate with shared search memory APIs. |
| `crates/search_verification/tests/*.rs` | `architecture_hash`, `DesignExperience`, `MemorySpace` | TEST_ONLY_USE | Migrate after production search verification imports. |
| `crates/engine/design_search_engine/tests/experiments/experience_prior.rs` | `DesignExperience`, `MemorySpace` | TEST_ONLY_USE | Migrate after design search engine production imports. |
| `crates/architecture_search/src/engine.rs` | `ArchitectureMetadata`, `DesignIntentRecord`, `DesignMemorySpace`, `ReasoningTrace`, `SearchStep`, `embed_architecture` | ARCHITECTURE_API_USE | Move to architecture memory extraction target, not `memory_engine`. |
| `crates/architecture_search/src/template_engine.rs` | `DesignIntentRecord`, `DesignMemorySpace`, `TemplateRecord`, `TopologyType`, `DependencyRuleRecord`, `TemplateMetadata` | ARCHITECTURE_API_USE | Move to architecture memory extraction target. |
| `crates/architecture_search/tests/core.rs` | `DesignMemorySpace` | TEST_ONLY_USE | Migrate after architecture search production imports. |
| `crates/architecture_evaluator/src/lib.rs` | `DesignMemorySpace`, evaluation record aliases, `embed_evaluation` | ARCHITECTURE_API_USE | Move to architecture memory or evaluator-owned memory crate. |
| `crates/architecture_evaluator/tests/ir_evaluation.rs` | `DesignMemorySpace` | TEST_ONLY_USE | Migrate after architecture evaluator production imports. |
| `crates/runtime/runtime_vm/src/adapter.rs` | `DesignExperience`, `InMemoryMemorySpace`, `MemorySpace`, `architecture_hash`, `layer_sequence_from_state` | EXPERIENCE_API_USE | Migrate with search memory APIs; verify runtime behavior. |
| `crates/runtime/runtime_vm/tests/*.rs` | `DesignIntentRecord`, `DesignMemorySpace`, `TemplateRecord`, `EvaluationRecord`, `embed_template` | TEST_ONLY_USE | Migrate after runtime VM production imports. |
| `crates/memory_space_phase14/tests/*.rs` | phase14 public APIs | TEST_ONLY_USE | Keep while phase14 owns APIs; remove with final package removal. |
| `crates/legacy/codegen_core_old/**/*.rs` | `memory_space_phase14::stable_v03` | LEGACY_ISOLATED | Out of scope; no active workspace graph dependency. |

## API Surface Inventory

| Symbol / Group | Current Path | Classification | Proposed Target |
|---|---|---|---|
| `PatternId`, `DesignPattern`, `PatternStore`, `MemorySpace`, `InMemoryMemorySpace`, `store_state_experience` | `crates/memory_space_phase14/src/pattern_store.rs` | PATTERN_API_USE | `search_memory_engine` or `design_search_engine` if ownership can be narrowed. |
| `PatternMatch`, `match_patterns` | `crates/memory_space_phase14/src/pattern_matcher.rs` | PATTERN_API_USE | `search_memory_engine` or `design_search_engine`. |
| `extract_pattern`, `layer_sequence_from_state`, `architecture_hash` | `crates/memory_space_phase14/src/pattern_extractor.rs` | PATTERN_API_USE | `search_memory_engine`; shared by runtime/search verification/runtime VM. |
| `SearchPrior` | `crates/memory_space_phase14/src/search_prior.rs` | SEARCH_API_USE | `search_memory_engine` or `design_search_engine`. |
| `DesignExperience`, `ExperienceStore` | `crates/memory_space_phase14/src/experience_store.rs` | EXPERIENCE_API_USE | `search_memory_engine`; do not move to `memory_engine` unless semantics are proven runtime-recall compatible. |
| `DesignMemorySpace`, `TemplateLearningEvent`, `embed_intent`, `embed_template`, `embed_architecture`, `embed_evaluation` | `crates/memory_space_phase14/src/MemorySpace/space.rs` | ARCHITECTURE_API_USE | `architecture_memory` or a dedicated architecture memory extraction target. |
| `TemplateRecord`, `TemplateMemoryDomain`, `TemplateMetadata`, `DependencyRuleRecord`, `TopologyType` | `crates/memory_space_phase14/src/MemorySpace/template_memory.rs` | ARCHITECTURE_API_USE | `architecture_memory`. |
| `ArchitectureRecord`, `ArchitectureMemoryDomain`, `ArchitectureMetadata` | `crates/memory_space_phase14/src/MemorySpace/architecture_memory.rs` | ARCHITECTURE_API_USE | `architecture_memory`. |
| `EvaluationRecord`, `EvaluationMemoryDomain`, `EvaluationScores`, `EvaluationMetricsV2`, `EvaluationDiagnostics` | `crates/memory_space_phase14/src/MemorySpace/evaluation_memory.rs` | ARCHITECTURE_API_USE | `architecture_memory` or evaluator memory crate. |
| `ReasoningTrace`, `ReasoningTraceMemoryDomain`, `SearchStep` | `crates/memory_space_phase14/src/MemorySpace/reasoning_trace_memory.rs` | SEARCH_API_USE | `search_memory_engine` if tied to search traces; otherwise `architecture_memory`. |
| `MemoryGraph`, `MemoryIndex`, `MemoryNode`, `MemoryEdge`, `MemoryMetadata`, `MemoryType`, `RelationType`, `MemoryId`, `DesignIntentRecord` | `crates/memory_space_phase14/src/MemorySpace/*` | ARCHITECTURE_API_USE | `architecture_memory`; review overlap with canonical `memory_space` first. |

## Consumer Classification

| Consumer | API Groups Used | Classification | Risk |
|---|---|---|---|
| `runtime_core` | search memory, experience prior | SEARCH_API_USE | Medium; search ranking and deterministic traces must remain stable. |
| `design_search_engine` | search memory, search prior, experience storage | SEARCH_API_USE | High; likely owner or anchor for extraction. |
| `policy_engine` | pattern model, experience model | PATTERN_API_USE | Medium; policy scoring depends on pattern identity stability. |
| `search_verification` | experience memory, architecture hash, pattern ids | EXPERIENCE_API_USE | Medium; verification determinism depends on exact hash/order behavior. |
| `architecture_search` | architecture memory, template memory, reasoning trace | ARCHITECTURE_API_USE | High; broad API surface and storage semantics. |
| `architecture_evaluator` | evaluation memory records and embedding | ARCHITECTURE_API_USE | Medium; evaluator output compatibility must be preserved. |
| `runtime_vm` | experience memory and architecture memory | EXPERIENCE_API_USE | High; spans runtime adapter plus architecture memory integration tests. |
| `memory_space_phase14` tests | owned API tests | TEST_ONLY_USE | Low; migrate or delete with owner crate split. |
| `crates/legacy/codegen_core_old` | removed `stable_v03` refs | LEGACY_ISOLATED | Not an active build blocker; separate legacy cleanup. |

## Proposed Migration Target

| API Group | Proposed Target | Decision |
|---|---|---|
| Search memory (`MemorySpace`, `InMemoryMemorySpace`, `SearchPrior`, `store_state_experience`) | `search_memory_engine` | Extract; used across runtime, design search, and verification. |
| Pattern APIs (`PatternId`, `DesignPattern`, `PatternStore`, `PatternMatch`, `match_patterns`) | `search_memory_engine` | Extract; shared by policy and search verification. |
| Experience APIs (`DesignExperience`, `ExperienceStore`) | `search_memory_engine` | Extract with search memory unless later proven suitable for `memory_engine`. |
| Pattern extraction/hash helpers (`architecture_hash`, `extract_pattern`, `layer_sequence_from_state`) | `search_memory_engine` | Extract; these are search/policy determinism utilities. |
| Architecture memory (`DesignMemorySpace`, records/domains, embeddings, template/evaluation/reasoning trace memory) | `architecture_memory` extraction target | Keep separate from `memory_engine`; likely a dedicated architecture memory plan is needed. |
| Legacy `stable_v03` references | none | Do not restore; handle in stale legacy cleanup. |

## Migration Risk

- Search ranking and beam behavior depend on `SearchPrior`, `store_state_experience`, and `InMemoryMemorySpace`; migration must preserve deterministic ordering.
- `PatternId` and `architecture_hash` are cross-crate identity contracts; changes can break policy and verification behavior.
- Architecture memory APIs are broader than search memory and should not be folded into `memory_engine`.
- `runtime_vm` mixes search experience APIs and architecture memory APIs, so it may need a staged migration.
- Legacy `codegen_core_old` already references removed `stable_v03`; this remains out of active graph and should stay outside this migration.

## Decision

Status: SEARCH_API_EXTRACTION_REQUIRED

The remaining `memory_space_phase14` API surface is active and split across at least two ownership domains:

- Search/pattern/experience memory should be extracted to `search_memory_engine`.
- Architecture/template/evaluation/reasoning-trace memory should be reviewed for an `architecture_memory` extraction path.

`memory_space_phase14` should be kept temporarily until those extraction plans are complete.

## Next Spec

- `DBM_SEARCH_MEMORY_ENGINE_EXTRACTION_PLAN_SPEC v1.0`
