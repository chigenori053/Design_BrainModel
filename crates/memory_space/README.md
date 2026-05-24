# memory_space

This crate is the canonical runtime MemorySpace implementation.

Policy:
- Preserve the existing `FileMemoryStore` storage format.
- Keep public API changes explicit and reviewed.
- Use `memory_space_core` and related crates for low-level utility boundaries.
