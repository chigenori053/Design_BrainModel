# Memory Space Store Naming Status

## Scope

This document records the current `crates/memory_space_legacy` store boundary after
deprecated store aliases were removed.

## Reference Inventory

Command:

```sh
grep -R "holographic_store" \
  crates apps Cargo.toml \
  --include="*.rs" \
  --include="*.toml" \
  -n
```

Expected result:

- no results

The old compatibility module is removed. It must not be reintroduced.

## Canonical Store API

The canonical names are:

- `MemoryStore`
- `FileMemoryStore`
- `MemoryEntry`
- `MemorySpace<S: MemoryStore = FileMemoryStore>`

Removed compatibility aliases:

- `LegacyMemoryStore`
- `LegacyStoreAdapter`
- `HolographicVectorStoreAdapter`

These aliases are no longer part of the public API. `MemoryStore` and
`FileMemoryStore` are the only store API names.

## Package Boundary

The package remains `memory_space`, and the directory remains
`crates/memory_space_legacy` for this phase. No Cargo package rename is part of
this cleanup.

## Verification Commands

```sh
cargo fmt
cargo test -p memory_space && cargo test -p dhm
cargo clippy -p memory_space -p dhm --all-targets -- -D warnings
```

Next specification: `DBM_MEMORY_SPACE_CRATE_BOUNDARY_REVIEW_SPEC v1.0`.
