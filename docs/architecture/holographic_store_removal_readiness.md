# Holographic Store Removal Readiness

## Scope

This document records whether `crates/memory_space_legacy/src/holographic_store.rs`
can be deleted after the adapter split and root public API removal work.

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
| `crates/memory_space_legacy/src/holographic_store.rs` | Allowed | Internal compatibility module delegating to `HolographicVectorStoreAdapter`. |
| `crates/memory_space_legacy/src/store_adapter.rs` | Allowed | Adapter definition and tests. No dependency on `HolographicVectorStore`. |
| `crates/memory_space_legacy/src/lib.rs` | Allowed | `pub mod holographic_store` remains for compatibility module tests; no root `HolographicVectorStore` re-export remains. |

No references remain in:

- `crates/dhm/src/lib.rs`
- `crates/memory_space_legacy/src/interference_memory.rs`
- `apps`
- other crate implementation files outside `memory_space_legacy`
- `Cargo.toml` dependency aliases

## Deletion Blockers

### Resolved blockers (as of `Split legacy holographic store blockers`)

1. **Adapter backend dependency** — resolved.
   `HolographicVectorStoreAdapter` no longer wraps `HolographicVectorStore`.
   It owns its own file I/O using the same `HVSTORE0` binary format.
   Dependency direction is now:
   ```
   holographic_store → store_adapter → memory_entry
   ```

2. **MemoryEntry location** — resolved.
   `MemoryEntry` has been moved to `crates/memory_space_legacy/src/memory_entry.rs`.
   It is re-exported from the crate root as `memory_space::MemoryEntry` without deprecation.
   `interference_memory.rs` imports it from `crate::memory_entry`.

### Resolved public API blocker

3. **Deprecated public re-export** — resolved.
   `lib.rs` no longer exposes:
   ```rust
   #[deprecated] pub use holographic_store::HolographicVectorStore;
   ```
   It still exposes the module while the file remains in place for the next
   removal step:
   ```rust
   pub mod holographic_store;
   ```

## Readiness Decision

Decision: **ready for removal candidate**.

Reason: the adapter is now independent of the legacy store, and `MemoryEntry` lives
in its own module. `holographic_store.rs` is a thin compatibility layer that fully
delegates to `HolographicVectorStoreAdapter`. `HolographicVectorStore` has been
removed from the crate root public API, while `HolographicVectorStoreAdapter`,
`LegacyMemoryStore`, and `MemoryEntry` remain root public API.

The compatibility file has not been deleted in this step. It is now an internal
compatibility module only and is ready to be considered for removal in the next
specification.

## Required Work Before Deletion

1. Remove `pub mod holographic_store` from `lib.rs`.
2. Delete `holographic_store.rs`.
3. Run full workspace reference checks, including downstream apps and docs.

## Verification Commands

```sh
# No bare HolographicVectorStore import in store_adapter.rs
grep -n "HolographicVectorStore[^A]" \
  crates/memory_space_legacy/src/store_adapter.rs || true
# Expected: (no output)

# No references in dhm or interference_memory
grep -n "HolographicVectorStore\|holographic_store" \
  crates/dhm/src/lib.rs \
  crates/memory_space_legacy/src/interference_memory.rs || true
# Expected: (no output)

# Tests and clippy pass
cargo test -p memory_space && cargo test -p dhm
cargo clippy -p memory_space -p dhm --all-targets -- -D warnings
```

Next specification: `DBM_MEMORY_SPACE_LEGACY_HOLOGRAPHIC_STORE_REMOVAL_SPEC v1.0`.
