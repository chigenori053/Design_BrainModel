# Memory Space Directory Rename Readiness

## Current State

- Current directory: `crates/memory_space_legacy`
- Proposed directory: `crates/memory_space`
- Package name: `memory_space`
- Public API impact: no public API change is intended for a directory-only
  rename.
- Storage format impact: no storage format change is intended.

## Reference Inventory

| Path | Reference Type | Required Action | Reason |
|---|---|---|---|
| `Cargo.toml` workspace members | RENAME_REQUIRED | Update member path from `crates/memory_space_legacy` after rename | Workspace member path points at the current directory. |
| `Cargo.toml` workspace dependency `memory_space` | RENAME_REQUIRED | Update dependency path after rename | The package name can remain `memory_space`, but its path would change. |
| `docs/architecture/holographic_store_removal_readiness.md` | DOC_UPDATE_ONLY | Update current directory references after rename | This is current architecture documentation. |
| `docs/architecture/memory_space_crate_boundary_review.md` | DOC_UPDATE_ONLY | Update directory naming debt references after rename | This review records the mismatch that the rename would resolve. |
| `docs/architecture/holographic_memory_inventory.md` | DOC_UPDATE_ONLY | Update `memory_space_legacy::store_adapter` wording after rename | This is current inventory documentation. |
| `docs/architecture/memoryspace_refactor_v2.md` | HISTORICAL_DOC_OK | Leave unchanged unless doing a broader doc refresh | It records an older refactor plan and historical naming. |
| `crates/memory_space_legacy/README.md` | RENAME_REQUIRED | Rename or rewrite during the actual directory rename | It is part of the crate directory and names the old directory. |
| `analyze.json` | UNKNOWN | Review before rename execution | Generated analysis data contains old source paths, including deleted historical files. |
| `.dbm/**` snapshots and session files | UNKNOWN | Review before rename execution | Generated/session state contains hard-coded old source paths; update policy is unclear. |
| `scripts/` | No reference found | None | Search found no matching references in the requested script scope. |
| `.github/` | No reference found | None | Search found no matching references in the requested CI scope. |
| `crates/` and `apps/` Rust/TOML files outside workspace `Cargo.toml` | No direct path reference found | None | Search found package-name imports, not hard-coded `crates/memory_space_legacy` paths. |

## Collision Check

| Check | Result |
|---|---|
| `find crates -maxdepth 2 -type d -name "memory_space"` | `crates/memory_space` exists |
| `find crates -maxdepth 2 -type d -name "memory_space_legacy"` | `crates/memory_space_legacy` exists |
| `grep -n '^name = "memory_space"' crates/memory_space_legacy/Cargo.toml` | `name = "memory_space"` |
| Existing `crates/memory_space/Cargo.toml` package name | `memory_space_phase14` |
| Workspace alias for `memory_space` | `memory_space = { path = "crates/memory_space_legacy" }` |
| Workspace alias for `memory_space_phase14` | `memory_space_phase14 = { path = "crates/memory_space" }` |

## Risk Assessment

| Risk | Severity | Mitigation |
|---|---|---|
| Proposed target directory already exists | High | Resolve `memory_space_phase14` directory/package placement before renaming `crates/memory_space_legacy`. |
| Two distinct package identities are path-inverted | High | Plan a multi-step rename that separates `memory_space` and `memory_space_phase14` directory moves. |
| Workspace dependency path must change | Medium | Update only workspace `Cargo.toml` path entries in the actual rename spec. |
| Current docs contain directory naming debt references | Low | Update current docs during actual rename; preserve historical docs when appropriate. |
| Generated `.dbm` and `analyze.json` files contain old paths | Medium | Decide whether these are regenerated, updated, or excluded before actual rename. |
| CI/scripts may reference old path | Low | Requested search found no `.github` or `scripts` references. Re-run before actual rename. |
| Runtime logic might use hard-coded path | Low | Requested Rust/TOML search found no runtime hard-coded `crates/memory_space_legacy` path outside workspace configuration. |

## Readiness Decision

Status: **BLOCKED**

Reason: `crates/memory_space` already exists and is the directory for the
`memory_space_phase14` package. A direct rename from `crates/memory_space_legacy`
to `crates/memory_space` would collide with an active workspace member. Package
name changes are not required for the canonical `memory_space` crate, and public
API/storage changes are not required, but the directory collision must be
resolved first.

## Next Spec

`DBM_MEMORY_SPACE_RENAME_BLOCKER_RESOLUTION_SPEC v1.0`

Recommended scope:

- Decide the target directory for the existing `memory_space_phase14` package.
- Define whether generated `.dbm` and `analyze.json` path snapshots should be
  updated, regenerated, or ignored.
- Only after the collision is resolved, run
  `DBM_MEMORY_SPACE_DIRECTORY_RENAME_SPEC v1.0`.
