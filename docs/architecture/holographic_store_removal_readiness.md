# Holographic Store Removal Readiness

## Scope

This document records whether `crates/memory_space_legacy/src/holographic_store.rs`
can be deleted after the adapter and public API deprecation work.

This step does not delete `holographic_store.rs`, remove `pub mod holographic_store`,
move `MemoryEntry`, or remove the deprecated public re-export.

## Reference Inventory

Command:

```sh
grep -R "HolographicVectorStore\|holographic_store" \
  crates apps Cargo.toml \
  --include="*.rs" \
  --include="*.toml" \
  -n
```

Remaining references:

| Location | Classification | Notes |
| --- | --- | --- |
| `crates/memory_space_legacy/src/holographic_store.rs` | Allowed | Legacy implementation and its local tests. |
| `crates/memory_space_legacy/src/store_adapter.rs` | Allowed | Adapter implementation wraps the legacy store. |
| `crates/memory_space_legacy/src/lib.rs` | Allowed | Deprecated compatibility re-export, public adapter exports, and compatibility boundary tests. |
| `crates/memory_space_legacy/src/interference_memory.rs` | Allowed for `holographic_store::MemoryEntry` only | Runtime store access is through `LegacyStoreAdapter`; `MemoryEntry` remains public compatibility data. |

No references were found in:

- `crates/dhm/src/lib.rs`
- `apps`
- other crate implementation files outside `memory_space_legacy`
- `Cargo.toml` dependency aliases

## Deletion Blockers

Current blocker status: blocked.

Blocking references:

- `store_adapter.rs` still depends on `holographic_store::{HolographicVectorStore, MemoryEntry}`.
- `lib.rs` still exposes `pub mod holographic_store`.
- `lib.rs` still has the deprecated `pub use holographic_store::HolographicVectorStore`.
- `MemoryEntry` is still defined in `holographic_store.rs` and is used by `store_adapter.rs`, `interference_memory.rs`, and public API tests.
- Legacy compatibility tests still instantiate or type-check `HolographicVectorStore`.

These blockers are intentional at this stage because compatibility is still required.

## Required Work Before Deletion

Before `holographic_store.rs` can be deleted:

1. Move `MemoryEntry` to a non-legacy module, or replace it with a new public data type.
2. Replace `HolographicVectorStoreAdapter` internals so they no longer wrap `HolographicVectorStore`.
3. Remove or rewrite adapter tests that compare against the legacy store implementation.
4. Remove the deprecated `HolographicVectorStore` public re-export after confirming external references are zero.
5. Remove `pub mod holographic_store` only in the actual removal step.
6. Run full workspace reference checks again, including downstream apps and docs.

## Readiness Decision

Decision: blocked.

Reason: the legacy implementation remains the storage backend behind
`HolographicVectorStoreAdapter`, and `MemoryEntry` still lives in
`holographic_store.rs`. The public API boundary has been narrowed, but the file is
not yet independently removable.

Next specification: `DBM_MEMORY_SPACE_LEGACY_HOLOGRAPHIC_STORE_BLOCKER_REMOVAL_SPEC v1.0`.
