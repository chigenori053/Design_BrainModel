//! Compatibility re-export for stable_v03 memory engine APIs.
//!
//! The implementation now lives in `memory_engine`.

pub use memory_engine::{
    CacheStats, InMemoryEngine, MemoryEdge, MemoryEngine, MemoryGraphSnapshot, MemoryNode,
    MemoryQuery, MemoryRecord, MemoryRelation, RecallConfig, RecallInput, RecallResult,
    RecalledRecord,
};
