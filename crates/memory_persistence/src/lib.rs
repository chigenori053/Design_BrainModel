//! memory_persistence — MemorySpace 永続化記憶の最適化システム
//!
//! 新しい記憶を取り込む際に以下のロジックを適用する:
//!
//! 1. **汎化 (Generalization)**: 記憶をより汎用的な形に抽象化して保存する
//! 2. **ユニーク判定 (Uniqueness check)**: 類似度が低い記憶は新規永続化記憶として保存する
//! 3. **アップグレード (Upgrade)**: 既存の汎化記憶と中程度の類似度の場合は既存記憶を更新する
//! 4. **重複排除 (Deduplication)**: 類似度が高い場合は保存しない

pub mod generalized_memory;
pub mod optimizer;
pub mod persistence_store;
pub mod similarity;

pub use generalized_memory::GeneralizedMemory;
pub use optimizer::{
    DUPLICATE_THRESHOLD, DecisionAction, DecisionEngine, DecisionEvidence, DecisionPolicy,
    IngestResult, SimilarityProfile, UNIQUE_THRESHOLD, UpgradeGate,
};
pub use persistence_store::{IngestAuditEntry, OptimizationStats, PersistentMemoryStore};
pub use similarity::{combined_similarity, cosine_similarity, jaccard_similarity};
