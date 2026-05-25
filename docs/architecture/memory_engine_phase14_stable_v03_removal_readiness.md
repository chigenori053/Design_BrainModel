# Memory Engine Phase14 Stable v03 Removal Readiness

## Current Status

Status: REMOVAL_READY_AFTER_COMPAT_TEST_DROP

`memory_engine` is the active runtime memory engine API owner. `memory_space_phase14::stable_v03` is deprecated and remains only as a compatibility re-export.

The remaining direct `memory_space_phase14::stable_v03` references are limited to:

- `memory_space_phase14` compatibility tests
- `crates/legacy/codegen_core_old` isolated legacy code

## Active Consumer Inventory

| Path | Reference | Classification | Decision |
|---|---|---|---|
| apps/*, active crates excluding `crates/legacy/codegen_core_old` | `memory_space_phase14::stable_v03` | ACTIVE_CONSUMER | No active consumer references found. |
| apps/*, active crates | `stable_v03::` | FALSE_POSITIVE | Matches unrelated `stable_v03` modules such as `runtime_core`, `code_language_core`, `architecture_ir`, and `design_search_engine`; not a phase14 stable_v03 blocker. |

## Test-only Reference Inventory

| Path | Reference | Classification | Decision |
|---|---|---|---|
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | `use memory_space_phase14::stable_v03::{...}` | TEST_ONLY_COMPAT | Drop or migrate this compatibility test before removing the module. |
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | `memory_space_phase14::stable_v03::MemoryEngine` | TEST_ONLY_COMPAT | Drop or migrate this compatibility assertion before removing the module. |
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | `memory_space_phase14::stable_v03::InMemoryEngine` | TEST_ONLY_COMPAT | Drop or migrate this compatibility assertion before removing the module. |
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | `memory_space_phase14::stable_v03::MemoryRecord` | TEST_ONLY_COMPAT | Drop or migrate this compatibility assertion before removing the module. |

## Isolated Legacy Reference Inventory

| Path | Reference | Classification | Decision |
|---|---|---|---|
| `crates/legacy/codegen_core_old/src/stable_v03.rs` | `use memory_space_phase14::stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` | ISOLATED_LEGACY | Not an active package graph blocker; handle under legacy removal/isolation work. |
| `crates/legacy/codegen_core_old/tests/phase4_dynamic_generation.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | ISOLATED_LEGACY | Not an active package graph blocker; handle under legacy removal/isolation work. |
| `crates/legacy/codegen_core_old/tests/profile_determinism.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | ISOLATED_LEGACY | Not an active package graph blocker; handle under legacy removal/isolation work. |
| `crates/legacy/codegen_core_old/tests/language_semantics.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | ISOLATED_LEGACY | Not an active package graph blocker; handle under legacy removal/isolation work. |

## Deprecated Allow Inventory

| Path | Classification | Decision |
|---|---|---|
| `crates/memory_space_phase14/tests/stable_v03_core.rs` | STABLE_V03_COMPAT_TEST | Valid compatibility-test-only allowance for the deprecated re-export. |
| `apps/cli/src/executor.rs` | UNRELATED_EXISTING_ALLOW | Existing unrelated deprecated allowance; not introduced for stable_v03. |
| `apps/cli/src/nl/executor.rs` | UNRELATED_EXISTING_ALLOW | Existing unrelated deprecated allowance; not introduced for stable_v03. |
| `apps/cli/src/nl/mod.rs` | UNRELATED_EXISTING_ALLOW | Existing unrelated deprecated allowance; not introduced for stable_v03. |
| `crates/hybrid_vm/src/lib.rs` | UNRELATED_EXISTING_ALLOW | Existing unrelated deprecated allowance; not introduced for stable_v03. |

## Cargo Dependency Impact

| Consumer | Dependency | Classification | Decision |
|---|---|---|---|
| root workspace | `memory_engine = { path = "crates/memory_engine" }` | WORKSPACE_DEPENDENCY | Keep. |
| root workspace | `memory_space_phase14 = { path = "crates/memory_space_phase14" }` | WORKSPACE_DEPENDENCY | Keep for remaining phase14 APIs. |
| `apps/cli` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep as runtime memory engine dependency. |
| `apps/cli` | `memory_space_phase14` | PHASE14_STABLE_V03_ONLY | Removal candidate after confirming no other phase14 API use. |
| `crates/runtime/runtime_core` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep as runtime memory engine dependency. |
| `crates/runtime/runtime_core` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; search controller still uses phase14 search/prior APIs. |
| `crates/memory_persistence` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep as runtime memory record dependency. |
| `crates/memory_persistence` | `memory_space_phase14` | PHASE14_STABLE_V03_ONLY | Removal candidate after dependency cleanup spec. |
| `crates/knowledge_engine` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep as runtime memory record dependency. |
| `crates/knowledge_engine` | `memory_space_phase14` | PHASE14_STABLE_V03_ONLY | Removal candidate after dependency cleanup spec. |
| `crates/code_language_core` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep as runtime memory engine dependency. |
| `crates/code_language_core` | `memory_space_phase14` | PHASE14_STABLE_V03_ONLY | Removal candidate after dependency cleanup spec. |
| `crates/memory_space_phase14` | `memory_engine` | MEMORY_ENGINE_ACTIVE | Keep while compatibility re-export exists. |
| `crates/policy_engine` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses pattern/experience APIs. |
| `crates/search_verification` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses experience/memory-space APIs. |
| `crates/engine/design_search_engine` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses search/prior APIs. |
| `crates/architecture_search` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses architecture memory APIs. |
| `crates/architecture_evaluator` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses architecture/design memory APIs. |
| `crates/runtime/runtime_vm` | `memory_space_phase14` | PHASE14_ACTIVE_OTHER_API | Keep; uses design memory APIs. |
| `crates/legacy/codegen_core_old` | `memory_space_phase14 = { path = "../../memory_space" }` | LEGACY_ISOLATED | Not in active graph; handle in legacy cleanup. |

## Active Package Graph

| Package / Path | In active graph | Decision |
|---|---:|---|
| `memory_engine` / `crates/memory_engine` | true | Active runtime memory engine API. |
| `memory_space_phase14` / `crates/memory_space_phase14` | true | Active for phase14 search/pattern/architecture APIs and deprecated stable_v03 compatibility. |
| `memory_space` / `crates/memory_space` | true | Canonical runtime memory crate. |
| `memory_space_core` / `crates/core/memory_space_core` | true | Active shared utility crate. |
| `crates/legacy/codegen_core_old` | false | Isolated legacy; not a stable_v03 removal blocker. |

## Removal Blockers

- `crates/memory_space_phase14/tests/stable_v03_core.rs` still validates the deprecated compatibility re-export.
- `crates/memory_space_phase14/src/stable_v03.rs` must remain until the compatibility test is dropped or migrated.
- `crates/legacy/codegen_core_old` still references `memory_space_phase14::stable_v03`, but it is outside the active package graph and is not a blocker for active build removal.

## Decision

- Active consumers have migrated to `memory_engine`.
- `memory_space_phase14::stable_v03` has no active consumer references.
- The deprecated compatibility module is removable after dropping or migrating the compatibility test.
- Isolated legacy references do not block removal from the active workspace graph.

Status: REMOVAL_READY_AFTER_COMPAT_TEST_DROP

## Next Spec

- `DBM_MEMORY_ENGINE_PHASE14_STABLE_V03_REMOVAL_SPEC v1.0`
