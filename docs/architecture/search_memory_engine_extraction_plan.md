# Search Memory Engine Extraction Plan

## Current Status

Status: PLAN_ONLY

`memory_engine` owns the runtime memory engine API. `memory_space_phase14` no longer contains `stable_v03`, but it still owns active search, pattern, and experience APIs that are used by runtime, design search, policy, verification, and runtime VM consumers.

The prior review classified these remaining APIs as requiring extraction rather than direct deletion.

## Extraction Target

Target crate:

- path: `crates/search_memory_engine`
- package: `search_memory_engine`

Role:

- search prior
- pattern identity
- pattern extraction
- pattern matching
- design experience storage
- deterministic search architecture hash helpers
- in-memory search memory implementation

Status for this spec:

- PLAN_ONLY
- no crate creation
- no API movement
- no dependency change
- no consumer import change

## API Extraction Inventory

| API Group | Symbols | Current Source | Proposed Owner | Action |
|---|---|---|---|---|
| Search memory | `MemorySpace`, `InMemoryMemorySpace`, `store_state_experience` | `crates/memory_space_phase14/src/pattern_store.rs` | `search_memory_engine` | Copy first, then re-export from phase14. |
| Search prior | `SearchPrior` | `crates/memory_space_phase14/src/search_prior.rs` | `search_memory_engine` | Preserve ranking behavior and deterministic ordering. |
| Pattern identity/model | `PatternId`, `DesignPattern`, `PatternStore` | `crates/memory_space_phase14/src/pattern_store.rs` | `search_memory_engine` | Preserve `PatternId` stability and pattern upsert behavior. |
| Pattern matching | `PatternMatch`, `match_patterns` | `crates/memory_space_phase14/src/pattern_matcher.rs` | `search_memory_engine` | Preserve match score/order behavior. |
| Pattern extraction/hash | `extract_pattern`, `layer_sequence_from_state`, `architecture_hash` | `crates/memory_space_phase14/src/pattern_extractor.rs` | `search_memory_engine` | Preserve hash identity exactly. |
| Experience model/store | `DesignExperience`, `ExperienceStore` | `crates/memory_space_phase14/src/experience_store.rs` | `search_memory_engine` | Preserve score filtering and sort behavior. |

## Non-Extraction Inventory

These APIs are not part of `search_memory_engine`; they belong to architecture memory planning.

| API Group | Symbols | Reason | Proposed Path |
|---|---|---|---|
| Architecture memory | `DesignMemorySpace`, `ArchitectureRecord`, `ArchitectureMemoryDomain`, `ArchitectureMetadata` | Stores architecture-domain records, not search memory. | Architecture memory extraction. |
| Intent/template memory | `DesignIntentRecord`, `TemplateRecord`, `TemplateMemoryDomain`, `TemplateMetadata`, `DependencyRuleRecord`, `TopologyType` | Template and architecture memory responsibility. | Architecture memory extraction. |
| Evaluation memory | `EvaluationRecord`, `EvaluationMemoryDomain`, `EvaluationScores`, `EvaluationMetricsV2`, `EvaluationDiagnostics` | Evaluator memory responsibility. | Architecture/evaluator memory extraction. |
| Reasoning trace memory | `ReasoningTrace`, `ReasoningTraceMemoryDomain`, `SearchStep` | Broader architecture/search trace storage; ownership needs separate review. | Architecture memory extraction review. |
| Embedding helpers | `embed_intent`, `embed_template`, `embed_architecture`, `embed_evaluation` | Coupled to architecture/template/evaluation records. | Architecture memory extraction. |

## Consumer Inventory

| Consumer | Current Phase14 Use | Classification | Proposed Migration |
|---|---|---|---|
| `runtime_core` | `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience` | Active search memory consumer | Migrate to `search_memory_engine` after re-export phase. |
| `design_search_engine` | `InMemoryMemorySpace`, `MemorySpace`, `SearchPrior`, `store_state_experience` | Active search memory consumer | Migrate early; likely anchor consumer for behavior checks. |
| `policy_engine` | `DesignPattern`, `PatternId`, `DesignExperience` | Active pattern/experience consumer | Migrate after pattern APIs are copied and tested. |
| `search_verification` | `DesignExperience`, `MemorySpace`, `PatternId`, `architecture_hash` | Active verification consumer | Migrate after deterministic hash compatibility is proven. |
| `runtime_vm` | `DesignExperience`, `InMemoryMemorySpace`, `MemorySpace`, `architecture_hash`, `layer_sequence_from_state` | Active experience/search consumer | Migrate in a staged pass because it also uses architecture memory APIs in tests. |
| `memory_space_phase14` tests | `DesignExperience`, `ExperienceStore`, `InMemoryMemorySpace`, `MemorySpace` | Test-only owner tests | Copy tests to `search_memory_engine`; keep phase14 tests through compatibility re-export phase. |
| `policy_engine` tests | `DesignExperience`, `DesignPattern`, `PatternId` | Test-only consumer | Migrate with `policy_engine`. |
| `search_verification` tests | `architecture_hash`, `DesignExperience`, `MemorySpace` | Test-only consumer | Migrate with `search_verification`; existing test failure is out of scope for this docs-only step. |
| `design_search_engine` tests | `DesignExperience`, `MemorySpace` | Test-only consumer | Migrate with `design_search_engine`; existing deep-search failure is out of scope for this docs-only step. |
| `runtime_vm` tests | Architecture memory APIs plus some search memory concepts | Mixed test-only consumer | Split search memory migration from architecture memory migration. |
| `architecture_search` | `DesignMemorySpace`, template and reasoning trace APIs | Architecture memory consumer | Out of scope for `search_memory_engine`; handle under architecture memory extraction. |
| `architecture_evaluator` | `DesignMemorySpace`, evaluation records, `embed_evaluation` | Architecture memory consumer | Out of scope for `search_memory_engine`. |
| `crates/legacy/codegen_core_old` | removed `memory_space_phase14::stable_v03` references | Legacy isolated | Out of scope; do not restore compatibility. |

## Proposed Crate Boundary

`search_memory_engine` should own only the search memory domain:

- pattern identity and pattern records
- in-memory pattern/experience store
- deterministic architecture hash for search experiences
- search prior construction
- pattern extraction and matching from `WorldState`
- design experience model used by policy/search consumers

It should not own:

- runtime recall memory (`memory_engine`)
- canonical memory-space graph/state APIs (`memory_space`)
- architecture template memory
- evaluation memory
- reasoning trace memory until ownership is clarified
- persistence format migration

## Migration Order

1. Phase 1: plan only
   - Current spec.
   - Add this document only.

2. Phase 2: crate scaffold
   - `DBM_SEARCH_MEMORY_ENGINE_CRATE_SCAFFOLD_SPEC v1.0`
   - Create `crates/search_memory_engine`.
   - Add workspace member and workspace dependency.
   - Add crate marker and scaffold test.

3. Phase 3: API copy
   - `DBM_SEARCH_MEMORY_ENGINE_API_COPY_SPEC v1.0`
   - Copy search, pattern, and experience APIs into `search_memory_engine`.
   - Copy phase14 tests into `search_memory_engine` with imports adjusted.
   - Do not modify phase14 implementation or consumers.

4. Phase 4: phase14 compatibility re-export
   - `DBM_SEARCH_MEMORY_ENGINE_PHASE14_COMPAT_REEXPORT_SPEC v1.0`
   - Replace phase14 search/pattern/experience implementations with re-exports from `search_memory_engine`.
   - Keep consumer imports unchanged.

5. Phase 5: consumer migration
   - `DBM_SEARCH_MEMORY_ENGINE_CONSUMER_MIGRATION_SPEC v1.0`
   - Migrate active consumer imports to `search_memory_engine`.
   - Add consumer Cargo dependencies.
   - Keep `memory_space_phase14` dependency where architecture memory APIs are still used.

6. Phase 6: phase14 search API deprecation
   - `DBM_SEARCH_MEMORY_ENGINE_PHASE14_DEPRECATION_SPEC v1.0`
   - Add deprecation to phase14 compatibility exports.
   - Restrict `allow(deprecated)` to compatibility tests only.

7. Phase 7: removal readiness
   - `DBM_SEARCH_MEMORY_ENGINE_PHASE14_REMOVAL_READINESS_SPEC v1.0`
   - Confirm no active consumer imports remain for phase14 search APIs.
   - Classify test-only and legacy references.

8. Phase 8: phase14 search API removal
   - `DBM_SEARCH_MEMORY_ENGINE_PHASE14_REMOVAL_SPEC v1.0`
   - Remove phase14 compatibility exports and compatibility tests.
   - Clean phase14 dependencies where no architecture memory API remains.

## Compatibility Strategy

- Use copy -> re-export -> consumer migration -> deprecation -> removal.
- Keep public names, fields, trait signatures, and function signatures unchanged during extraction.
- Preserve deterministic behavior for:
  - `architecture_hash`
  - `PatternId`
  - `PatternStore` upsert and next-id behavior
  - `SearchPrior` ranking and weighting behavior
  - `ExperienceStore` score filtering and ordering
- Move tests before changing consumers so behavior can be compared in both owners.
- Do not combine this with architecture memory extraction.

## Risk Assessment

High:

- `architecture_hash` changes affect `search_verification`, policy learning, and `runtime_vm`.
- `SearchPrior` changes affect beam search convergence and state ranking.
- `PatternId` instability affects policy scoring and deterministic verification.

Medium:

- `DesignExperience` and `ExperienceStore` migration can alter search-prior reproducibility.
- `runtime_vm` mixes search memory and architecture memory APIs, so its migration must be staged.
- Existing search tests have known failures unrelated to this docs-only step; these must be handled before using them as strict migration gates.

Low:

- Crate scaffold and marker tests.
- Plan-only documentation.

## Decision

Status: PLAN_ONLY

The extraction target is confirmed as `search_memory_engine` for search, pattern, and experience APIs. Architecture memory APIs are explicitly excluded and require a separate extraction path.

## Next Spec

- `DBM_SEARCH_MEMORY_ENGINE_CRATE_SCAFFOLD_SPEC v1.0`
