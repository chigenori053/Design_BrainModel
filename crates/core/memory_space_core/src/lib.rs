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
    AliasNodeSnapshot, CanonicalNodeSnapshot, CanonicalReferenceMap, CausalLinkId,
    ClusterCandidate, ClusterCandidateSnapshot, ClusterCandidateTable, ClusterId, CollapseRisk,
    DecayPolicy, DedupEvent, DedupInsertResult, DriftResistance, FingerprintComparison,
    HolographicDeduplicationManager, IdentityLineage, InvariantCore, MemoryAccessProfile,
    MemoryIdentity, MemoryLifecycle, MemoryNode, ObservationGraph, ObservationThreshold,
    ReplayFingerprint, RewriteEnergy, SemanticAnchor, SemanticAttractor, SemanticAttractorField,
    SemanticAttractorSnapshot, SemanticCluster, SemanticCompressionOperation,
    SemanticCompressionSnapshot, SemanticCoreCandidate, SemanticCoreCandidateTable,
    SemanticCoreSnapshot, SemanticCorrectionPlan, SemanticDistance, SemanticDistanceSnapshot,
    SemanticDistanceWeights, SemanticDrift, SemanticDriftEvent, SemanticDriftSnapshot,
    SemanticFingerprint, SemanticFragment, SemanticIdentity, SemanticIdentityCandidate,
    SemanticIdentityGraph, SemanticIdentityId, SemanticIdentitySnapshot, SemanticMergeCandidate,
    SemanticMergeResult, SemanticObservation, SemanticObservationSnapshot, SemanticRelation,
    SemanticRewritePlan, SemanticRewritePreview, SemanticRewriteTransaction,
    SemanticRewriteValidation, SemanticRollbackSnapshot, SemanticStability,
    SemanticStabilitySnapshot, SemanticStabilizationState, SemanticTopologyDiff, StabilityVelocity,
    StabilityWindow, StateTrajectory, TemporalClusterSnapshot, TemporalObservation, TopologyDiff,
    TopologySnapshot, TrajectoryId, TrajectorySnapshot, TransitionId, apply_semantic_correction,
    build_cluster_candidates, build_observation_graph, build_semantic_fingerprint,
    cluster_candidate_snapshot, collapse_risk, compare_fingerprint, compose_total_distance,
    compute_candidate_coherence, continuity_score, detect_semantic_drift,
    deterministic_rewrite_checksum, diff_snapshots, drift_resistance, fingerprint_hash,
    identity_lineages, merge_candidates, merge_risk_score, normalize_distance,
    observe_semantic_relation, record_temporal_cluster_snapshot, record_temporal_observation,
    rewrite_energy, semantic_attractor_field, semantic_attractor_snapshot, semantic_attractors,
    semantic_compression, semantic_core_candidates, semantic_core_snapshot,
    semantic_correction_plan, semantic_distance, semantic_distance_snapshot, semantic_drift,
    semantic_drift_snapshot, semantic_identity_graph, semantic_identity_snapshot, semantic_merge,
    semantic_observation_snapshot, semantic_rewrite_preview, semantic_rewrite_transaction,
    semantic_rollback_snapshot, semantic_signature_from_tokens, semantic_stability,
    semantic_stability_snapshot, serialize_snapshot, snapshot_hash, stability_velocity,
    stability_window, stabilization_state, validate_semantic_rewrite,
};
pub use memory::{Complex64, MemoryField, MemoryId};
pub use memory_engine::MemoryEngine;
pub use modality::{AudioBuffer, ImageBuffer, ModalityInput, ModalityKind};
pub use recall::{RecallConfig, RecallQuery, RecallResult};
pub use store::{InMemoryMemoryStore, MemoryRecord};
pub use traits::MemoryStore;
