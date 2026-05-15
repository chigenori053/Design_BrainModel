pub mod candidate;
pub mod errors;
pub mod feature_index;
pub mod holographic_dedup;
pub mod memory;
pub mod memory_engine;
pub mod modality;
pub mod recall;
pub mod store;
pub mod traits;

pub use candidate::{MemoryCandidate, RecallCandidate};
pub use errors::MemorySpaceError;
pub use feature_index::FeatureIndex;
pub use holographic_dedup::{
    AliasNodeSnapshot, CanonicalNodeSnapshot, CanonicalReferenceMap, CausalLinkId, DecayPolicy,
    DedupEvent, DedupInsertResult, HolographicDeduplicationManager, MemoryAccessProfile,
    MemoryIdentity, MemoryLifecycle, ReplayFingerprint, SemanticCluster, StateTrajectory,
    TopologyDiff, TopologySnapshot, TrajectoryId, TrajectorySnapshot, TransitionId, diff_snapshots,
    semantic_signature_from_tokens, serialize_snapshot, snapshot_hash,
};
pub use memory::{Complex64, MemoryField, MemoryId};
pub use memory_engine::MemoryEngine;
pub use modality::{AudioBuffer, ImageBuffer, ModalityInput, ModalityKind};
pub use recall::{RecallConfig, RecallQuery, RecallResult};
pub use store::{InMemoryMemoryStore, MemoryRecord};
pub use traits::MemoryStore;
