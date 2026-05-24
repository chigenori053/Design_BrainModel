# Holographic Memory Inventory

## memory_space_core::holographic_dedup
Canonical deduplication / identity-lineage manager.
Do not delete.

## design_reasoning::holographic_semantic_memory
Semantic reasoning memory and concept synthesis support.
Do not delete unless semantic_concept_synthesis is migrated.

## memory_space_legacy::store_adapter
Canonical file-backed store boundary for the frozen MemorySpace v1 package.
Use `MemoryStore` and `FileMemoryStore`; old store names are deprecated,
migration-only aliases and are the next removal candidate.
