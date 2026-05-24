# Memory Space Directory Rename Readiness

## Current State

- Current directory: `crates/memory_space`
- Previous directory: legacy runtime path
- Package name: `memory_space`
- Collision blocker: resolved by moving the `memory_space_phase14` package to
  `crates/memory_space_phase14`.
- Directory rename: completed.
- Public API impact: no public API change is intended for a directory-only
  rename.
- Storage format impact: no storage format change is intended.

## Reference Inventory

| Path | Reference Type | Required Action | Reason |
|---|---|---|---|
| `Cargo.toml` workspace members | RENAME_DONE | Member path now points at `crates/memory_space` | Workspace member path has been updated. |
| `Cargo.toml` workspace dependency `memory_space` | RENAME_DONE | Dependency path now points at `crates/memory_space` | The package name remained `memory_space`. |
| `docs/architecture/holographic_store_removal_readiness.md` | DOC_UPDATE_ONLY | Update current directory references after rename | This is current architecture documentation. |
| `docs/architecture/memory_space_crate_boundary_review.md` | DOC_UPDATE_ONLY | Update directory naming debt references after rename | This review records the mismatch that the rename would resolve. |
| `docs/architecture/holographic_memory_inventory.md` | DOC_UPDATE_ONLY | Update `memory_space::store_adapter` wording after rename | This is current inventory documentation. |
| `docs/architecture/memoryspace_refactor_v2.md` | HISTORICAL_DOC_OK | Leave unchanged unless doing a broader doc refresh | It records an older refactor plan and historical naming. |
| `crates/memory_space/README.md` | RENAME_DONE | Rewritten during the directory rename | It is part of the crate directory and now names the canonical crate. |
| `analyze.json` | RENAME_DONE | Updated tracked generated path references | This tracked generated file contained old source paths. |
| `.dbm/**` snapshots and session files | GENERATED_UNTRACKED | Leave uncommitted | Generated/session state contains old source paths but is not tracked. |
| `scripts/` | No reference found | None | Search found no matching references in the requested script scope. |
| `.github/` | No reference found | None | Search found no matching references in the requested CI scope. |
| `crates/` and `apps/` Rust/TOML files outside workspace `Cargo.toml` | No direct path reference found | None | Search found package-name imports, not hard-coded directory paths. |

## Collision Check

| Check | Result |
|---|---|
| `find crates -maxdepth 2 -type d -name "memory_space"` | `crates/memory_space` exists |
| Previous canonical runtime directory | removed |
| `find crates -maxdepth 2 -type d -name "memory_space_phase14"` | `crates/memory_space_phase14` exists |
| `grep -n '^name = "memory_space"' crates/memory_space/Cargo.toml` | `name = "memory_space"` |
| Existing `crates/memory_space_phase14/Cargo.toml` package name | `memory_space_phase14` |
| Workspace alias for `memory_space` | `memory_space = { path = "crates/memory_space" }` |
| Workspace alias for `memory_space_phase14` | `memory_space_phase14 = { path = "crates/memory_space_phase14" }` |

## Risk Assessment

| Risk | Severity | Mitigation |
|---|---|---|
| Proposed target directory already exists | Resolved | `memory_space_phase14` has moved to `crates/memory_space_phase14`, leaving `crates/memory_space` available. |
| Two distinct package identities are path-inverted | Resolved | The phase14 package path is now aligned with its package name. |
| Workspace dependency path must change | Medium | Update only workspace `Cargo.toml` path entries in the actual rename spec. |
| Current docs contain directory naming debt references | Low | Update current docs during actual rename; preserve historical docs when appropriate. |
| Generated `.dbm` and `analyze.json` files contain old paths | Low | Tracked `analyze.json` was updated; untracked `.dbm/**` snapshots are left out of the commit. |
| CI/scripts may reference old path | Low | Requested search found no `.github` or `scripts` references. Re-run before actual rename. |
| Runtime logic might use hard-coded path | Low | Requested Rust/TOML search found no runtime hard-coded directory path outside workspace configuration. |

## Readiness Decision

Status: **READY**

Reason: the canonical runtime crate has been renamed to `crates/memory_space`;
the `memory_space_phase14` package remains at `crates/memory_space_phase14`.
Package name changes, public API changes, and storage changes were not required.

## Next Spec

`DBM_MEMORY_SPACE_CORE_BOUNDARY_ALIGNMENT_SPEC v1.0`

Recommended scope:

- Keep `memory_space` as the canonical runtime MemorySpace / FileMemoryStore
  crate.
- Keep `memory_space_core` as low-level utility / dedup / identity support.
- Keep `memory_space_phase14` as experimental implementation pending separate
  review.
- Keep generated `.dbm/**` session snapshots out of the commit unless a later
  workflow requires regeneration.
