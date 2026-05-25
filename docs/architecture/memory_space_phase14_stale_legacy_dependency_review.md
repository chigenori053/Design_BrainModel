# Memory Space Phase14 Stale Legacy Dependency Review

## Current Structure

Previous plan:

- Spec: `DBM_MEMORY_SPACE_PHASE14_DEPENDENCY_MIGRATION_PLAN_SPEC v1.0`
- Result: `PLAN_ONLY`
- Document: `docs/architecture/memory_space_phase14_dependency_migration_plan.md`

Current crate structure:

- `memory_space`
  - path: `crates/memory_space`
  - package: `memory_space`
  - status: VALID_CANONICAL_RUNTIME
- `memory_space_phase14`
  - path: `crates/memory_space_phase14`
  - package: `memory_space_phase14`
  - status: VALID_EXPERIMENTAL
- `crates/memory_space_legacy`
  - status: ABSENT

Workspace dependency paths are valid:

- `memory_space = { path = "crates/memory_space" }`
- `memory_space_phase14 = { path = "crates/memory_space_phase14" }`

## Workspace Membership

| Package / Path | Status | Classification | Action |
|---|---|---|---|
| `crates/memory_space` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Keep as canonical runtime memory. |
| `crates/memory_space_phase14` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Keep as experimental phase14 memory during migration planning. |
| `crates/core/memory_space_core` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Keep as core utility crate. |
| `crates/memory_space_complex` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Not stale; name contains `memory_space` but is a separate active utility crate. |
| `crates/memory_space_api` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Not stale; active memory API crate. |
| `crates/memory_space_eval` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Not stale; active memory evaluation crate. |
| `crates/memory_space_index` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Not stale; active memory index crate. |
| `crates/memory_space_recall` | Workspace member and package in `cargo metadata` | VALID_PACKAGE_DEFINITION | Not stale; active memory recall crate. |
| `crates/legacy/codegen_core_old` | Not listed in root `Cargo.toml` workspace members and absent from `cargo metadata` package list | STALE_LEGACY_ISOLATED | Review deletion, isolation, or dependency repair in a later legacy-focused spec. |
| `crates/memory_space_legacy` | Directory absent | NO_REFERENCE | No action. |

## Stale Path Dependency Inventory

| File | Reference | Classification | Action |
|---|---|---|---|
| `Cargo.toml` | `memory_space_phase14 = { path = "crates/memory_space_phase14" }` | VALID_WORKSPACE_DEPENDENCY | Keep. |
| `Cargo.toml` | `memory_space = { path = "crates/memory_space" }` | VALID_WORKSPACE_DEPENDENCY | Keep. |
| `crates/memory_space/Cargo.toml` | `name = "memory_space"` | VALID_PACKAGE_DEFINITION | Keep. |
| `crates/memory_space_phase14/Cargo.toml` | `name = "memory_space_phase14"` | VALID_PACKAGE_DEFINITION | Keep. |
| `crates/legacy/codegen_core_old/Cargo.toml` | `memory_space_phase14 = { path = "../../memory_space" }` | STALE_DIRECT_DEPENDENCY | Do not change in this review; classify as isolated because the crate is outside the active workspace graph. |
| `docs/architecture/memory_space_core_boundary_alignment.md` | `memory_space_phase14 = { path = "../../memory_space" }` | STALE_DOC_ONLY | Keep as historical review evidence unless a docs cleanup spec decides otherwise. |
| `docs/architecture/memory_space_phase14_deprecation_review.md` | `memory_space_phase14 = { path = "../../memory_space" }` | STALE_DOC_ONLY | Keep as prior review finding. |
| `docs/architecture/memory_space_phase14_dependency_migration_plan.md` | `memory_space_phase14 = { path = "../../memory_space" }` | STALE_DOC_ONLY | Keep as prior plan blocker. |
| `docs/architecture/*` | Current path mentions such as `crates/memory_space`, `crates/memory_space_phase14`, and workspace dependency examples | VALID_DOC_REFERENCE | Keep; these record current valid structure or previous review findings. |

The requested stale path dependency scan for `*.toml` returned only the valid
root workspace phase14 dependency. The legacy stale dependency uses a relative
path (`../../memory_space`) and was found by the dedicated legacy crate scan.

## Legacy Crate Inventory

| Crate / Path | Workspace Member | Memory Dependency | Classification | Action |
|---|---:|---|---|---|
| `crates/legacy/codegen_core_old` | No | `memory_space_phase14 = { path = "../../memory_space" }` | STALE_LEGACY_ISOLATED | Review separately; it is not an active workspace package but contains a dependency name/path mismatch. |

No other `Cargo.toml` files were found under `crates/legacy`.

## Rust Import Inventory

| File | Import | Classification | Action |
|---|---|---|---|
| `crates/legacy/codegen_core_old/src/stable_v03.rs` | `use memory_space_phase14::stable_v03::{MemoryEngine, MemoryQuery, MemoryRecord}` | STALE_LEGACY_USE | Do not change here; resolve with legacy stale dependency decision. |
| `crates/legacy/codegen_core_old/tests/phase4_dynamic_generation.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | STALE_LEGACY_USE | Do not change here; crate is outside active workspace graph. |
| `crates/legacy/codegen_core_old/tests/profile_determinism.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | STALE_LEGACY_USE | Do not change here; crate is outside active workspace graph. |
| `crates/legacy/codegen_core_old/tests/language_semantics.rs` | `use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord}` | STALE_LEGACY_USE | Do not change here; crate is outside active workspace graph. |
| `apps/cli/**` | `memory_space_phase14::stable_v03::*` | VALID_ACTIVE_USE | Not stale; tracked by the memory engine extraction plan. |
| `crates/runtime/runtime_core/**` | `memory_space_phase14::stable_v03::*` and search memory imports | VALID_ACTIVE_USE | Not stale; tracked by memory engine and search memory extraction plans. |
| `crates/runtime/runtime_vm/**` | `memory_space_phase14::{DesignMemorySpace, DesignIntentRecord, TemplateRecord, EvaluationRecord, embed_template}` | VALID_ACTIVE_USE | Not stale; tracked by architecture memory extraction plan. |
| `crates/search_verification/**` | `memory_space_phase14::{DesignExperience, MemorySpace, PatternId, architecture_hash}` | VALID_ACTIVE_USE | Not stale; tracked by search memory extraction plan. |
| `crates/engine/design_search_engine/**` | `memory_space_phase14::{InMemoryMemorySpace, MemorySpace, SearchPrior, store_state_experience}` | VALID_ACTIVE_USE | Not stale; tracked by search memory extraction plan. |
| `crates/architecture_search/**` | `memory_space_phase14::{DesignIntentRecord, DesignMemorySpace, TemplateRecord, TopologyType}` and related records | VALID_ACTIVE_USE | Not stale; tracked by architecture memory extraction plan. |
| `crates/architecture_evaluator/**` | Phase14 design memory records and `DesignMemorySpace` | VALID_ACTIVE_USE | Not stale; tracked by architecture memory extraction plan. |
| `crates/policy_engine/**` | `memory_space_phase14::{DesignExperience, DesignPattern, PatternId}` | VALID_ACTIVE_USE | Not stale; tracked by search memory extraction plan. |
| `crates/memory_persistence/**` | `memory_space_phase14::stable_v03::MemoryRecord` | VALID_ACTIVE_USE | Not stale; tracked by memory engine extraction plan. |
| `crates/knowledge_engine/**` | `memory_space_phase14::stable_v03::*` | VALID_ACTIVE_USE | Not stale; tracked by memory engine extraction plan. |
| `crates/code_language_core/**` | `memory_space_phase14::stable_v03::*` | VALID_ACTIVE_USE | Not stale; tracked by memory engine extraction plan. |
| Active `memory_space::*` imports in `agent_core`, `field_engine`, `dhm`, `hybrid_vm`, `shm`, and related crates | Canonical `memory_space` API imports | VALID_ACTIVE_USE | Not stale; canonical runtime imports are valid. |
| Active `memory_space_core::*`, `memory_space_complex::*`, `memory_space_api::*`, `memory_space_index::*`, `memory_space_recall::*`, `memory_space_eval::*` imports | Current memory utility/API crates | VALID_ACTIVE_USE | Not stale; names match active workspace crates. |

## Active Build Impact

- `cargo metadata --no-deps --format-version 1` includes `memory_space`,
  `memory_space_phase14`, `memory_space_core`, and other active
  `memory_space_*` utility crates.
- `cargo metadata` does not include `codegen_core_old`.
- The root workspace member list does not include `crates/legacy/codegen_core_old`.
- There is no `exclude` entry for `crates/legacy/codegen_core_old`; the crate is
  simply not listed as a workspace member.
- The stale legacy dependency therefore does not enter the active workspace
  package graph for normal workspace builds.
- Active phase14 imports in CLI/runtime/search/policy/persistence/knowledge/code
  consumers are valid active uses, not stale dependencies. They remain covered
  by the phase14 dependency migration plan.

## Risk Assessment

- The active build is not currently affected by a stale direct dependency on
  `crates/memory_space_legacy` or by a workspace member that misroutes
  `memory_space_phase14` to `crates/memory_space`.
- The stale dependency in `crates/legacy/codegen_core_old` can still confuse
  future repo-wide dependency rewrites, migration scripts, or manual audits.
- The legacy crate imports `memory_space_phase14::stable_v03`, while its
  dependency declaration points to `../../memory_space`; if someone builds that
  crate directly, the dependency name/path mismatch is likely to fail or produce
  misleading diagnostics.
- Prior architecture docs intentionally record this stale legacy finding. Those
  references are documentation evidence, not active build dependencies.

## Decision

Status: STALE_LEGACY_ISOLATED

Rationale:

- `crates/memory_space_legacy` is absent.
- Valid workspace dependencies point to `crates/memory_space` and
  `crates/memory_space_phase14`.
- No active workspace package was found with a stale direct path dependency.
- `crates/legacy/codegen_core_old` contains a stale direct dependency and stale
  legacy imports, but it is outside root workspace membership and absent from
  `cargo metadata`.
- Active phase14 consumers are not stale; they are migration-plan targets.

## Next Spec

DBM_MEMORY_ENGINE_EXTRACTION_PLAN_SPEC v1.0
