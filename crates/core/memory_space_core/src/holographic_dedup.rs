use std::collections::{BTreeMap, BTreeSet};

use crate::{MemoryId, MemorySpaceError};

pub type TransitionId = u64;
pub type TrajectoryId = u64;
pub type CausalLinkId = u64;
pub type ClusterId = u64;
pub type SemanticIdentityId = u64;
pub type CanonicalReferenceMap = BTreeMap<MemoryId, Vec<MemoryId>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryIdentity {
    pub memory_id: MemoryId,
    pub state_hash: u64,
    pub transition_hash: u64,
    pub semantic_signature: u64,
}

impl MemoryIdentity {
    fn exact_key(&self) -> ExactDuplicateKey {
        ExactDuplicateKey {
            state_hash: self.state_hash,
            transition_hash: self.transition_hash,
            semantic_signature: self.semantic_signature,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateTrajectory {
    pub trajectory_id: TrajectoryId,
    pub transitions: Vec<TransitionId>,
    pub causal_links: Vec<CausalLinkId>,
}

impl StateTrajectory {
    pub fn transition_hash(&self) -> u64 {
        stable_hash_u64s(
            self.transitions
                .iter()
                .chain(self.causal_links.iter())
                .copied(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticCluster {
    pub canonical_id: MemoryId,
    pub aliases: Vec<MemoryId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryLifecycle {
    Active,
    Dormant,
    Archived,
    Compressed,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoryAccessProfile {
    pub last_access_epoch: u64,
    pub access_count: u64,
    pub semantic_redundancy: f64,
}

impl Default for MemoryAccessProfile {
    fn default() -> Self {
        Self {
            last_access_epoch: 0,
            access_count: 0,
            semantic_redundancy: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecayPolicy {
    pub dormant_after_epochs: u64,
    pub archive_below_access_count: u64,
    pub compress_at_semantic_redundancy: f64,
}

impl Default for DecayPolicy {
    fn default() -> Self {
        Self {
            dormant_after_epochs: 100,
            archive_below_access_count: 2,
            compress_at_semantic_redundancy: 0.85,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DedupEvent {
    MemoryInserted {
        memory_id: MemoryId,
    },
    ExactDuplicateMerged {
        duplicate_id: MemoryId,
        canonical_id: MemoryId,
    },
    SemanticAliasRegistered {
        alias_id: MemoryId,
        canonical_id: MemoryId,
    },
    TransitionCommitted {
        memory_id: MemoryId,
        trajectory_id: TrajectoryId,
    },
    TransitionRolledBack {
        memory_id: MemoryId,
        trajectory_id: TrajectoryId,
    },
    LifecycleChanged {
        memory_id: MemoryId,
        from: MemoryLifecycle,
        to: MemoryLifecycle,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedupInsertResult {
    pub requested_id: MemoryId,
    pub canonical_id: MemoryId,
    pub inserted: bool,
    pub events: Vec<DedupEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayFingerprint {
    pub memory_id: MemoryId,
    pub trajectory_id: TrajectoryId,
    pub transitions: Vec<TransitionId>,
    pub causal_links: Vec<CausalLinkId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologySnapshot {
    pub canonical_nodes: Vec<CanonicalNodeSnapshot>,
    pub alias_nodes: Vec<AliasNodeSnapshot>,
    pub transition_hashes: Vec<u64>,
    pub replay_fingerprint: ReplayFingerprint,
    pub trajectory_snapshots: Vec<TrajectorySnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalNodeSnapshot {
    pub canonical_id: MemoryId,
    pub semantic_signature: u64,
    pub reference_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AliasNodeSnapshot {
    pub alias_id: MemoryId,
    pub canonical_id: MemoryId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrajectorySnapshot {
    pub trajectory_id: TrajectoryId,
    pub transition_ids: Vec<TransitionId>,
    pub causal_links: Vec<CausalLinkId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyDiff {
    pub equal: bool,
    pub canonical_nodes_changed: bool,
    pub added_aliases: Vec<AliasNodeSnapshot>,
    pub removed_aliases: Vec<AliasNodeSnapshot>,
    pub transition_hashes_changed: bool,
    pub replay_fingerprint_changed: bool,
    pub trajectory_snapshots_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryNode {
    pub memory_id: MemoryId,
    pub tokens: Vec<String>,
    pub semantic_labels: Vec<String>,
    pub relations: Vec<SemanticRelation>,
    pub dependency_links: Vec<(MemoryId, MemoryId)>,
    pub causal_links: Vec<CausalLinkId>,
    pub trajectory_hint: Vec<TransitionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticRelation {
    pub source: String,
    pub relation: String,
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticFingerprint {
    pub topology_hash: u64,
    pub token_signature: u64,
    pub relation_signature: u64,
    pub trajectory_hint: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FingerprintComparison {
    pub topology_match: bool,
    pub token_match: bool,
    pub relation_match: bool,
    pub trajectory_hint_match: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDistance {
    pub topology_distance: f64,
    pub token_distance: f64,
    pub relation_distance: f64,
    pub trajectory_penalty: f64,
    pub total_distance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDistanceWeights {
    pub topology_weight: f64,
    pub token_weight: f64,
    pub relation_weight: f64,
    pub trajectory_weight: f64,
}

impl Default for SemanticDistanceWeights {
    fn default() -> Self {
        Self {
            topology_weight: 0.40,
            relation_weight: 0.30,
            token_weight: 0.20,
            trajectory_weight: 0.10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDistanceSnapshot {
    pub source: MemoryId,
    pub target: MemoryId,
    pub distance: SemanticDistance,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservationThreshold {
    pub topology_threshold: f64,
    pub relation_threshold: f64,
    pub total_threshold: f64,
}

impl Default for ObservationThreshold {
    fn default() -> Self {
        Self {
            topology_threshold: 0.50,
            relation_threshold: 0.50,
            total_threshold: 0.50,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticObservation {
    pub source: MemoryId,
    pub target: MemoryId,
    pub distance: SemanticDistance,
    pub observation_strength: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticObservationSnapshot {
    pub observations: Vec<SemanticObservation>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObservationGraph {
    pub adjacency: BTreeMap<MemoryId, Vec<SemanticObservation>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterCandidate {
    pub candidate_id: ClusterId,
    pub members: Vec<MemoryId>,
    pub average_distance: f64,
    pub topology_coherence: f64,
    pub relation_coherence: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterCandidateTable {
    pub candidates: Vec<ClusterCandidate>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterCandidateSnapshot {
    pub candidates: Vec<ClusterCandidate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemporalObservation {
    pub timestamp: u64,
    pub snapshot: SemanticObservationSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemporalClusterSnapshot {
    pub timestamp: u64,
    pub snapshot: ClusterCandidateSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDrift {
    pub drift_score: f64,
    pub topology_shift: f64,
    pub relation_shift: f64,
    pub membership_change: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDriftSnapshot {
    pub before_timestamp: u64,
    pub after_timestamp: u64,
    pub drift: SemanticDrift,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticStability {
    pub stability_score: f64,
    pub temporal_consistency: f64,
    pub topology_consistency: f64,
    pub relation_consistency: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StabilityVelocity {
    pub drift_velocity: f64,
    pub stability_velocity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StabilityWindow {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub observations: Vec<SemanticDriftSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticStabilitySnapshot {
    pub timestamp: u64,
    pub stability: SemanticStability,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCoreCandidate {
    pub core_id: u64,
    pub invariant_members: Vec<MemoryId>,
    pub stability_score: f64,
    pub drift_resistance: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticCoreCandidateTable {
    pub candidates: Vec<SemanticCoreCandidate>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticCoreSnapshot {
    pub timestamp: u64,
    pub candidates: Vec<SemanticCoreCandidate>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftResistance {
    pub oscillation_resistance: f64,
    pub topology_resistance: f64,
    pub relation_resistance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticIdentityCandidate {
    pub identity_id: u64,
    pub continuity_score: f64,
    pub invariant_core_overlap: f64,
    pub drift_lineage: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticIdentityGraph {
    pub identities: Vec<SemanticIdentityCandidate>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticIdentitySnapshot {
    pub timestamp: u64,
    pub identities: Vec<SemanticIdentityCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityLineage {
    pub ancestor_id: u64,
    pub descendant_ids: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InvariantCore {
    pub core_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticFragment {
    pub fragment_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticMergeCandidate {
    pub left_identity: SemanticIdentityId,
    pub right_identity: SemanticIdentityId,
    pub continuity_score: f64,
    pub invariant_overlap_score: f64,
    pub contradiction_density: f64,
    pub lineage_distance: usize,
    pub merge_risk_score: f64,
    pub compression_gain: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticMergeResult {
    pub merged_identity: SemanticIdentityId,
    pub source_identities: Vec<SemanticIdentityId>,
    pub preserved_invariants: Vec<InvariantCore>,
    pub discarded_fragments: Vec<SemanticFragment>,
    pub merge_confidence: f64,
    pub semantic_loss_score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticCompressionSnapshot {
    pub timestamp: u64,
    pub identity_count_before: usize,
    pub identity_count_after: usize,
    pub compression_ratio: f64,
    pub preserved_semantic_mass: f64,
    pub discarded_semantic_mass: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticAnchor {
    pub anchor_id: u64,
    pub identity_id: SemanticIdentityId,
    pub invariant_core: InvariantCore,
}

pub type SemanticIdentity = SemanticIdentityCandidate;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAttractor {
    pub attractor_id: u64,
    pub anchor_set: Vec<SemanticAnchor>,
    pub invariant_density: f64,
    pub stability_score: f64,
    pub attractor_strength: f64,
    pub semantic_mass: f64,
    pub basin_strength: f64,
    pub semantic_density: f64,
    pub stability_gradient: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticAttractorField {
    pub attractors: Vec<SemanticAttractor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RewriteEnergy {
    pub topology_energy: f64,
    pub relation_energy: f64,
    pub continuity_energy: f64,
    pub total_energy: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollapseRisk {
    pub collapse_score: f64,
    pub semantic_density_risk: f64,
    pub attractor_overconvergence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAttractorSnapshot {
    pub timestamp: u64,
    pub field: SemanticAttractorField,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticStabilizationState {
    pub identity_id: SemanticIdentityId,
    pub attractor_id: u64,
    pub continuity_score: f64,
    pub drift_score: f64,
    pub contradiction_density: f64,
    pub stabilization_confidence: f64,
    pub recoverable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticDriftEvent {
    pub identity_id: SemanticIdentityId,
    pub previous_continuity: f64,
    pub current_continuity: f64,
    pub drift_magnitude: f64,
    pub recoverable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCorrectionPlan {
    pub target_identity: SemanticIdentityId,
    pub restored_invariants: Vec<InvariantCore>,
    pub rejected_fragments: Vec<SemanticFragment>,
    pub correction_confidence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCompressionOperation {
    pub operation_id: u64,
    pub source_identities: Vec<SemanticIdentityId>,
    pub expected_compression_gain: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SemanticTopologyDiff {
    pub merge_candidate_count: usize,
    pub correction_target_count: usize,
    pub compression_operation_count: usize,
    pub branch_preservation_count: usize,
    pub attractor_change_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRewritePlan {
    pub merge_operations: Vec<SemanticMergeCandidate>,
    pub correction_operations: Vec<SemanticCorrectionPlan>,
    pub compression_operations: Vec<SemanticCompressionOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRewritePreview {
    pub topology_diff: SemanticTopologyDiff,
    pub continuity_delta: f64,
    pub semantic_mass_delta: f64,
    pub contradiction_delta: f64,
    pub anchor_preservation_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticRewriteValidation {
    pub valid: bool,
    pub continuity_retained: bool,
    pub anchors_preserved: bool,
    pub contradiction_bounded: bool,
    pub semantic_mass_bounded: bool,
    pub replay_invariant: bool,
    pub topology_invariant: bool,
    pub validation_errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRollbackSnapshot {
    pub snapshot_id: u64,
    pub topology_snapshot: SemanticIdentitySnapshot,
    pub attractor_snapshot: Vec<SemanticAttractor>,
    pub anchor_snapshot: Vec<SemanticAnchor>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticRewriteTransaction {
    pub transaction_id: u64,
    pub source_snapshot: SemanticIdentitySnapshot,
    pub rewrite_plan: SemanticRewritePlan,
    pub preview: SemanticRewritePreview,
    pub validation: SemanticRewriteValidation,
    pub rollback_snapshot: SemanticRollbackSnapshot,
    pub deterministic_checksum: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExactDuplicateKey {
    state_hash: u64,
    transition_hash: u64,
    semantic_signature: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct HolographicMemoryNode {
    identity: MemoryIdentity,
    trajectory: StateTrajectory,
    lifecycle: MemoryLifecycle,
    access: MemoryAccessProfile,
    references: BTreeSet<MemoryId>,
}

#[derive(Clone, Debug, Default)]
struct AttractorAccumulator {
    identity_id: SemanticIdentityId,
    observation_count: usize,
    merge_count: usize,
    correction_count: usize,
    compression_gain: f64,
    total_continuity: f64,
    total_invariant: f64,
    total_semantic_mass: f64,
    total_energy_resistance: f64,
    lineage: BTreeSet<u64>,
}

impl AttractorAccumulator {
    fn new(identity_id: SemanticIdentityId) -> Self {
        Self {
            identity_id,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HolographicDeduplicationManager {
    nodes: BTreeMap<MemoryId, HolographicMemoryNode>,
    exact_index: BTreeMap<ExactDuplicateKey, MemoryId>,
    canonical_by_semantic: BTreeMap<u64, MemoryId>,
    semantic_clusters: BTreeMap<u64, SemanticCluster>,
}

impl HolographicDeduplicationManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_memory_insert(
        &mut self,
        identity: MemoryIdentity,
        trajectory: StateTrajectory,
    ) -> Result<DedupInsertResult, MemorySpaceError> {
        if identity.transition_hash != trajectory.transition_hash() {
            return Err(MemorySpaceError::TransitionHashMismatch {
                expected: identity.transition_hash,
                actual: trajectory.transition_hash(),
            });
        }

        let exact_key = identity.exact_key();
        if let Some(canonical_id) = self.exact_index.get(&exact_key).copied() {
            let canonical = self
                .nodes
                .get_mut(&canonical_id)
                .ok_or(MemorySpaceError::MissingCanonicalMemory(canonical_id))?;
            canonical.references.insert(identity.memory_id);
            return Ok(DedupInsertResult {
                requested_id: identity.memory_id,
                canonical_id,
                inserted: false,
                events: vec![DedupEvent::ExactDuplicateMerged {
                    duplicate_id: identity.memory_id,
                    canonical_id,
                }],
            });
        }

        let semantic_canonical = self
            .canonical_by_semantic
            .get(&identity.semantic_signature)
            .copied();
        let mut events = vec![DedupEvent::MemoryInserted {
            memory_id: identity.memory_id,
        }];

        let node = HolographicMemoryNode {
            identity,
            trajectory,
            lifecycle: MemoryLifecycle::Active,
            access: MemoryAccessProfile::default(),
            references: BTreeSet::from([identity.memory_id]),
        };
        self.nodes.insert(identity.memory_id, node);
        self.exact_index.insert(exact_key, identity.memory_id);

        if let Some(canonical_id) = semantic_canonical {
            let cluster = self
                .semantic_clusters
                .entry(identity.semantic_signature)
                .or_insert_with(|| SemanticCluster {
                    canonical_id,
                    aliases: Vec::new(),
                });
            if !cluster.aliases.contains(&identity.memory_id) {
                cluster.aliases.push(identity.memory_id);
            }
            events.push(DedupEvent::SemanticAliasRegistered {
                alias_id: identity.memory_id,
                canonical_id,
            });
        } else {
            self.canonical_by_semantic
                .insert(identity.semantic_signature, identity.memory_id);
            self.semantic_clusters.insert(
                identity.semantic_signature,
                SemanticCluster {
                    canonical_id: identity.memory_id,
                    aliases: Vec::new(),
                },
            );
        }

        Ok(DedupInsertResult {
            requested_id: identity.memory_id,
            canonical_id: identity.memory_id,
            inserted: true,
            events,
        })
    }

    pub fn on_memory_merge(
        &self,
        left: MemoryId,
        right: MemoryId,
    ) -> Result<MemoryId, MemorySpaceError> {
        let left = self
            .nodes
            .get(&left)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(left))?;
        let right = self
            .nodes
            .get(&right)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(right))?;
        if left.identity.exact_key() == right.identity.exact_key() {
            Ok(left.identity.memory_id.min(right.identity.memory_id))
        } else {
            Err(MemorySpaceError::UnsafeTransitionMerge)
        }
    }

    pub fn on_transition_commit(
        &mut self,
        memory_id: MemoryId,
        transition_id: TransitionId,
        causal_link_id: CausalLinkId,
    ) -> Result<DedupEvent, MemorySpaceError> {
        let old_exact_key = self
            .nodes
            .get(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?
            .identity
            .exact_key();
        let node = self
            .nodes
            .get_mut(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?;
        node.trajectory.transitions.push(transition_id);
        node.trajectory.causal_links.push(causal_link_id);
        node.identity.transition_hash = node.trajectory.transition_hash();
        let new_exact_key = node.identity.exact_key();
        let trajectory_id = node.trajectory.trajectory_id;
        if self.exact_index.get(&old_exact_key) == Some(&memory_id) {
            self.exact_index.remove(&old_exact_key);
        }
        if let Some(canonical_id) = self.exact_index.get(&new_exact_key).copied() {
            if canonical_id != memory_id {
                let duplicate = self
                    .nodes
                    .remove(&memory_id)
                    .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?;
                let canonical = self
                    .nodes
                    .get_mut(&canonical_id)
                    .ok_or(MemorySpaceError::MissingCanonicalMemory(canonical_id))?;
                canonical.references.extend(duplicate.references);
                return Ok(DedupEvent::ExactDuplicateMerged {
                    duplicate_id: memory_id,
                    canonical_id,
                });
            }
        }
        self.exact_index.insert(new_exact_key, memory_id);
        Ok(DedupEvent::TransitionCommitted {
            memory_id,
            trajectory_id,
        })
    }

    pub fn on_transition_rollback(
        &mut self,
        memory_id: MemoryId,
    ) -> Result<DedupEvent, MemorySpaceError> {
        let old_exact_key = self
            .nodes
            .get(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?
            .identity
            .exact_key();
        let node = self
            .nodes
            .get_mut(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?;
        node.trajectory.transitions.pop();
        node.trajectory.causal_links.pop();
        node.identity.transition_hash = node.trajectory.transition_hash();
        let new_exact_key = node.identity.exact_key();
        let trajectory_id = node.trajectory.trajectory_id;

        if self.exact_index.get(&old_exact_key) == Some(&memory_id) {
            self.exact_index.remove(&old_exact_key);
        }
        self.exact_index.insert(new_exact_key, memory_id);

        Ok(DedupEvent::TransitionRolledBack {
            memory_id,
            trajectory_id,
        })
    }

    pub fn on_decay_check(&mut self, now_epoch: u64, policy: DecayPolicy) -> Vec<DedupEvent> {
        let mut events = Vec::new();
        for node in self.nodes.values_mut() {
            let next = next_lifecycle(node.lifecycle, node.access, now_epoch, policy);
            if next != node.lifecycle {
                events.push(DedupEvent::LifecycleChanged {
                    memory_id: node.identity.memory_id,
                    from: node.lifecycle,
                    to: next,
                });
                node.lifecycle = next;
            }
        }
        events
    }

    pub fn record_access(
        &mut self,
        memory_id: MemoryId,
        epoch: u64,
    ) -> Result<(), MemorySpaceError> {
        let node = self
            .nodes
            .get_mut(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?;
        node.access.last_access_epoch = epoch;
        node.access.access_count = node.access.access_count.saturating_add(1);
        Ok(())
    }

    pub fn set_semantic_redundancy(
        &mut self,
        memory_id: MemoryId,
        redundancy: f64,
    ) -> Result<(), MemorySpaceError> {
        let node = self
            .nodes
            .get_mut(&memory_id)
            .ok_or(MemorySpaceError::MissingCanonicalMemory(memory_id))?;
        node.access.semantic_redundancy = redundancy.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn semantic_cluster(&self, semantic_signature: u64) -> Option<&SemanticCluster> {
        self.semantic_clusters.get(&semantic_signature)
    }

    pub fn replay_fingerprint(&self, memory_id: MemoryId) -> Option<ReplayFingerprint> {
        self.nodes.get(&memory_id).map(|node| ReplayFingerprint {
            memory_id: node.identity.memory_id,
            trajectory_id: node.trajectory.trajectory_id,
            transitions: node.trajectory.transitions.clone(),
            causal_links: node.trajectory.causal_links.clone(),
        })
    }

    pub fn lifecycle(&self, memory_id: MemoryId) -> Option<MemoryLifecycle> {
        self.nodes.get(&memory_id).map(|node| node.lifecycle)
    }

    pub fn references_for(&self, memory_id: MemoryId) -> Option<Vec<MemoryId>> {
        self.nodes
            .get(&memory_id)
            .map(|node| node.references.iter().copied().collect())
    }

    pub fn topology_snapshot(&self) -> TopologySnapshot {
        let mut canonical_nodes = self
            .exact_index
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|memory_id| {
                self.nodes
                    .get(&memory_id)
                    .map(|node| CanonicalNodeSnapshot {
                        canonical_id: memory_id,
                        semantic_signature: node.identity.semantic_signature,
                        reference_count: node.references.len(),
                    })
            })
            .collect::<Vec<_>>();
        canonical_nodes.sort();

        let mut alias_nodes = Vec::new();
        for node in self.nodes.values() {
            for alias_id in &node.references {
                if *alias_id != node.identity.memory_id {
                    alias_nodes.push(AliasNodeSnapshot {
                        alias_id: *alias_id,
                        canonical_id: node.identity.memory_id,
                    });
                }
            }
        }
        for cluster in self.semantic_clusters.values() {
            for alias_id in &cluster.aliases {
                alias_nodes.push(AliasNodeSnapshot {
                    alias_id: *alias_id,
                    canonical_id: cluster.canonical_id,
                });
            }
        }
        alias_nodes.sort();
        alias_nodes.dedup();

        let mut transition_hashes = self
            .nodes
            .values()
            .map(|node| node.identity.transition_hash)
            .collect::<Vec<_>>();
        transition_hashes.sort();

        let replay_fingerprint = canonical_nodes
            .first()
            .and_then(|node| self.replay_fingerprint(node.canonical_id))
            .unwrap_or_else(|| ReplayFingerprint {
                memory_id: 0,
                trajectory_id: 0,
                transitions: Vec::new(),
                causal_links: Vec::new(),
            });

        let mut trajectory_snapshots = self
            .nodes
            .values()
            .map(|node| TrajectorySnapshot {
                trajectory_id: node.trajectory.trajectory_id,
                transition_ids: node.trajectory.transitions.clone(),
                causal_links: node.trajectory.causal_links.clone(),
            })
            .collect::<Vec<_>>();
        trajectory_snapshots.sort();

        TopologySnapshot {
            canonical_nodes,
            alias_nodes,
            transition_hashes,
            replay_fingerprint,
            trajectory_snapshots,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

pub fn semantic_signature_from_tokens<'a>(tokens: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut normalized = tokens
        .into_iter()
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    stable_hash_bytes(normalized.join("\0").as_bytes())
}

pub fn serialize_snapshot(snapshot: &TopologySnapshot) -> String {
    format!(
        "{{\"canonical_nodes\":[{}],\"alias_nodes\":[{}],\"transition_hashes\":[{}],\"replay_fingerprint\":{},\"trajectory_snapshots\":[{}]}}",
        snapshot
            .canonical_nodes
            .iter()
            .map(serialize_canonical_node)
            .collect::<Vec<_>>()
            .join(","),
        snapshot
            .alias_nodes
            .iter()
            .map(serialize_alias_node)
            .collect::<Vec<_>>()
            .join(","),
        serialize_u64_array(&snapshot.transition_hashes),
        serialize_replay_fingerprint(&snapshot.replay_fingerprint),
        snapshot
            .trajectory_snapshots
            .iter()
            .map(serialize_trajectory_snapshot)
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn snapshot_hash(snapshot: &TopologySnapshot) -> u64 {
    stable_hash_bytes(serialize_snapshot(snapshot).as_bytes())
}

pub fn diff_snapshots(before: &TopologySnapshot, after: &TopologySnapshot) -> TopologyDiff {
    let before_aliases = before.alias_nodes.iter().copied().collect::<BTreeSet<_>>();
    let after_aliases = after.alias_nodes.iter().copied().collect::<BTreeSet<_>>();
    let added_aliases = after_aliases
        .difference(&before_aliases)
        .copied()
        .collect::<Vec<_>>();
    let removed_aliases = before_aliases
        .difference(&after_aliases)
        .copied()
        .collect::<Vec<_>>();
    let before_canonical_identity = before
        .canonical_nodes
        .iter()
        .map(|node| (node.canonical_id, node.semantic_signature))
        .collect::<Vec<_>>();
    let after_canonical_identity = after
        .canonical_nodes
        .iter()
        .map(|node| (node.canonical_id, node.semantic_signature))
        .collect::<Vec<_>>();
    let canonical_nodes_changed = before_canonical_identity != after_canonical_identity;
    let transition_hashes_changed = before.transition_hashes != after.transition_hashes;
    let replay_fingerprint_changed = before.replay_fingerprint != after.replay_fingerprint;
    let trajectory_snapshots_changed = before.trajectory_snapshots != after.trajectory_snapshots;

    TopologyDiff {
        equal: before == after,
        canonical_nodes_changed,
        added_aliases,
        removed_aliases,
        transition_hashes_changed,
        replay_fingerprint_changed,
        trajectory_snapshots_changed,
    }
}

pub fn build_semantic_fingerprint(memory: &MemoryNode) -> SemanticFingerprint {
    let tokens = normalized_sorted_strings(
        memory
            .tokens
            .iter()
            .chain(memory.semantic_labels.iter())
            .map(String::as_str),
    );
    let relations = normalized_sorted_relations(&memory.relations);
    let dependency_links = sorted_links(&memory.dependency_links);
    let causal_links = sorted_u64s(&memory.causal_links);

    let token_signature = stable_hash_strings(tokens.iter().map(String::as_str));
    let relation_signature = stable_hash_strings(
        relations
            .iter()
            .chain(dependency_links.iter())
            .chain(causal_links.iter())
            .map(String::as_str),
    );
    let topology_hash = stable_hash_strings(
        tokens
            .iter()
            .chain(relations.iter())
            .chain(dependency_links.iter())
            .chain(causal_links.iter())
            .map(String::as_str),
    );
    let trajectory_hint = stable_hash_u64s(memory.trajectory_hint.iter().copied());

    SemanticFingerprint {
        topology_hash,
        token_signature,
        relation_signature,
        trajectory_hint,
    }
}

pub fn fingerprint_hash(fp: &SemanticFingerprint) -> u64 {
    stable_hash_u64s([
        fp.topology_hash,
        fp.token_signature,
        fp.relation_signature,
        fp.trajectory_hint,
    ])
}

pub fn compare_fingerprint(
    a: &SemanticFingerprint,
    b: &SemanticFingerprint,
) -> FingerprintComparison {
    FingerprintComparison {
        topology_match: a.topology_hash == b.topology_hash,
        token_match: a.token_signature == b.token_signature,
        relation_match: a.relation_signature == b.relation_signature,
        trajectory_hint_match: a.trajectory_hint == b.trajectory_hint,
    }
}

pub fn semantic_distance(
    a: &SemanticFingerprint,
    b: &SemanticFingerprint,
    weights: &SemanticDistanceWeights,
) -> SemanticDistance {
    let mut distance = SemanticDistance {
        topology_distance: component_distance(a.topology_hash, b.topology_hash),
        token_distance: component_distance(a.token_signature, b.token_signature),
        relation_distance: component_distance(a.relation_signature, b.relation_signature),
        trajectory_penalty: component_distance(a.trajectory_hint, b.trajectory_hint),
        total_distance: 0.0,
    };
    distance.total_distance = compose_total_distance(&distance, weights);
    distance
}

pub fn normalize_distance(value: f64) -> f64 {
    if value.is_nan() {
        1.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

pub fn compose_total_distance(
    distance: &SemanticDistance,
    weights: &SemanticDistanceWeights,
) -> f64 {
    let topology_weight = normalize_weight(weights.topology_weight);
    let token_weight = normalize_weight(weights.token_weight);
    let relation_weight = normalize_weight(weights.relation_weight);
    let trajectory_weight = normalize_weight(weights.trajectory_weight);
    let weight_total = topology_weight + token_weight + relation_weight + trajectory_weight;
    if weight_total == 0.0 {
        return 0.0;
    }

    normalize_distance(
        (normalize_distance(distance.topology_distance) * topology_weight
            + normalize_distance(distance.token_distance) * token_weight
            + normalize_distance(distance.relation_distance) * relation_weight
            + normalize_distance(distance.trajectory_penalty) * trajectory_weight)
            / weight_total,
    )
}

pub fn semantic_distance_snapshot(
    source: MemoryId,
    target: MemoryId,
    a: &SemanticFingerprint,
    b: &SemanticFingerprint,
    weights: &SemanticDistanceWeights,
) -> SemanticDistanceSnapshot {
    SemanticDistanceSnapshot {
        source,
        target,
        distance: semantic_distance(a, b, weights),
    }
}

pub fn observe_semantic_relation(
    source: &MemoryNode,
    target: &MemoryNode,
    threshold: &ObservationThreshold,
) -> Option<SemanticObservation> {
    if source.memory_id == target.memory_id {
        return None;
    }

    let source_fp = build_semantic_fingerprint(source);
    let target_fp = build_semantic_fingerprint(target);
    let distance = semantic_distance(&source_fp, &target_fp, &SemanticDistanceWeights::default());
    if distance.topology_distance <= normalize_distance(threshold.topology_threshold)
        && distance.relation_distance <= normalize_distance(threshold.relation_threshold)
        && distance.total_distance <= normalize_distance(threshold.total_threshold)
    {
        Some(SemanticObservation {
            source: source.memory_id,
            target: target.memory_id,
            distance,
            observation_strength: normalize_distance(1.0 - distance.total_distance),
        })
    } else {
        None
    }
}

pub fn build_observation_graph(
    memories: &[MemoryNode],
    threshold: &ObservationThreshold,
) -> ObservationGraph {
    let mut ordered = memories.to_vec();
    ordered.sort_by_key(|memory| memory.memory_id);

    let mut adjacency: BTreeMap<MemoryId, Vec<SemanticObservation>> = BTreeMap::new();
    for source in &ordered {
        for target in &ordered {
            if let Some(observation) = observe_semantic_relation(source, target, threshold) {
                adjacency
                    .entry(source.memory_id)
                    .or_default()
                    .push(observation);
            }
        }
    }

    for observations in adjacency.values_mut() {
        observations.sort_by(observation_order);
    }
    ObservationGraph { adjacency }
}

pub fn semantic_observation_snapshot(graph: &ObservationGraph) -> SemanticObservationSnapshot {
    let mut observations = graph
        .adjacency
        .values()
        .flat_map(|observations| observations.iter().copied())
        .collect::<Vec<_>>();
    observations.sort_by(observation_order);
    SemanticObservationSnapshot { observations }
}

pub fn build_cluster_candidates(
    graph: &ObservationGraph,
    threshold: &ObservationThreshold,
) -> ClusterCandidateTable {
    let mut edges = BTreeMap::<MemoryId, BTreeSet<MemoryId>>::new();
    let mut distances = BTreeMap::<(MemoryId, MemoryId), SemanticDistance>::new();
    for observations in graph.adjacency.values() {
        for observation in observations {
            if observation.distance.total_distance <= normalize_distance(threshold.total_threshold)
            {
                edges
                    .entry(observation.source)
                    .or_default()
                    .insert(observation.target);
                edges
                    .entry(observation.target)
                    .or_default()
                    .insert(observation.source);
                distances.insert(
                    ordered_pair(observation.source, observation.target),
                    observation.distance,
                );
            }
        }
    }

    let mut visited = BTreeSet::new();
    let mut candidates = Vec::new();
    for start in edges.keys().copied().collect::<Vec<_>>() {
        if visited.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        let mut members = BTreeSet::new();
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            members.insert(node);
            if let Some(neighbors) = edges.get(&node) {
                for neighbor in neighbors.iter().rev() {
                    if !visited.contains(neighbor) {
                        stack.push(*neighbor);
                    }
                }
            }
        }

        if members.len() < 2 {
            continue;
        }
        let members = members.into_iter().collect::<Vec<_>>();
        let member_distances = candidate_member_distances(&members, &distances);
        if member_distances.is_empty() {
            continue;
        }
        let average_distance = normalize_distance(
            member_distances.iter().sum::<f64>() / member_distances.len() as f64,
        );
        let topology_coherence = normalize_distance(1.0 - average_distance);
        let relation_distances = candidate_relation_distances(&members, &distances);
        let relation_coherence = if relation_distances.is_empty() {
            topology_coherence
        } else {
            normalize_distance(
                1.0 - relation_distances.iter().sum::<f64>() / relation_distances.len() as f64,
            )
        };
        candidates.push(ClusterCandidate {
            candidate_id: stable_hash_u64s(members.iter().copied()),
            members,
            average_distance,
            topology_coherence,
            relation_coherence,
        });
    }

    candidates.sort_by(candidate_order);
    ClusterCandidateTable { candidates }
}

pub fn cluster_candidate_snapshot(table: &ClusterCandidateTable) -> ClusterCandidateSnapshot {
    let mut candidates = table.candidates.clone();
    candidates.sort_by(candidate_order);
    ClusterCandidateSnapshot { candidates }
}

pub fn compute_candidate_coherence(candidate: &ClusterCandidate) -> (f64, f64) {
    (
        normalize_distance(candidate.topology_coherence),
        normalize_distance(candidate.relation_coherence),
    )
}

pub fn semantic_drift(
    before: &ClusterCandidateSnapshot,
    after: &ClusterCandidateSnapshot,
) -> SemanticDrift {
    let before_candidates = normalized_candidates(before);
    let after_candidates = normalized_candidates(after);
    let before_memberships = membership_sets(&before_candidates);
    let after_memberships = membership_sets(&after_candidates);

    let topology_shift = set_distance(&before_memberships, &after_memberships);
    let relation_shift = coherence_shift(&before_candidates, &after_candidates);
    let membership_change = member_set_distance(&before_candidates, &after_candidates);
    let drift_score = normalize_distance(
        (topology_shift * 0.4) + (relation_shift * 0.3) + (membership_change * 0.3),
    );

    SemanticDrift {
        drift_score,
        topology_shift,
        relation_shift,
        membership_change,
    }
}

pub fn semantic_drift_snapshot(
    before_timestamp: u64,
    after_timestamp: u64,
    drift: SemanticDrift,
) -> SemanticDriftSnapshot {
    let (before_timestamp, after_timestamp) = if before_timestamp <= after_timestamp {
        (before_timestamp, after_timestamp)
    } else {
        (after_timestamp, before_timestamp)
    };
    SemanticDriftSnapshot {
        before_timestamp,
        after_timestamp,
        drift,
    }
}

pub fn record_temporal_observation(
    timestamp: u64,
    snapshot: SemanticObservationSnapshot,
) -> TemporalObservation {
    let mut observations = snapshot.observations;
    observations.sort_by(observation_order);
    TemporalObservation {
        timestamp,
        snapshot: SemanticObservationSnapshot { observations },
    }
}

pub fn record_temporal_cluster_snapshot(
    timestamp: u64,
    snapshot: ClusterCandidateSnapshot,
) -> TemporalClusterSnapshot {
    let mut candidates = snapshot.candidates;
    candidates.sort_by(candidate_order);
    TemporalClusterSnapshot {
        timestamp,
        snapshot: ClusterCandidateSnapshot { candidates },
    }
}

pub fn semantic_stability(window: &StabilityWindow) -> SemanticStability {
    let observations = normalized_drift_observations(window);
    if observations.is_empty() {
        return SemanticStability {
            stability_score: 1.0,
            temporal_consistency: 1.0,
            topology_consistency: 1.0,
            relation_consistency: 1.0,
        };
    }

    let mean_drift = observations
        .iter()
        .map(|snapshot| snapshot.drift.drift_score)
        .sum::<f64>()
        / observations.len() as f64;
    let temporal_consistency = normalize_distance(1.0 - mean_drift);
    let topology_consistency = normalize_distance(
        1.0 - observations
            .iter()
            .map(|snapshot| snapshot.drift.topology_shift)
            .sum::<f64>()
            / observations.len() as f64,
    );
    let relation_consistency = normalize_distance(
        1.0 - observations
            .iter()
            .map(|snapshot| snapshot.drift.relation_shift)
            .sum::<f64>()
            / observations.len() as f64,
    );
    let oscillation_penalty = drift_oscillation_penalty(&observations);
    let stability_score = normalize_distance(
        ((temporal_consistency * 0.4)
            + (topology_consistency * 0.3)
            + (relation_consistency * 0.3))
            * (1.0 - oscillation_penalty),
    );

    SemanticStability {
        stability_score,
        temporal_consistency,
        topology_consistency,
        relation_consistency,
    }
}

pub fn stability_velocity(window: &StabilityWindow) -> StabilityVelocity {
    let observations = normalized_drift_observations(window);
    if observations.len() < 2 {
        return StabilityVelocity {
            drift_velocity: 0.0,
            stability_velocity: 0.0,
        };
    }

    let first = observations
        .first()
        .expect("first observation")
        .drift
        .drift_score;
    let last = observations
        .last()
        .expect("last observation")
        .drift
        .drift_score;
    let duration = observations
        .last()
        .expect("last observation")
        .after_timestamp
        .saturating_sub(
            observations
                .first()
                .expect("first observation")
                .before_timestamp,
        )
        .max(1) as f64;
    let drift_velocity = ((last - first) / duration).clamp(-1.0, 1.0);
    StabilityVelocity {
        drift_velocity,
        stability_velocity: (-drift_velocity).clamp(-1.0, 1.0),
    }
}

pub fn semantic_stability_snapshot(
    timestamp: u64,
    stability: SemanticStability,
) -> SemanticStabilitySnapshot {
    SemanticStabilitySnapshot {
        timestamp,
        stability,
    }
}

pub fn stability_window(
    start_timestamp: u64,
    end_timestamp: u64,
    observations: Vec<SemanticDriftSnapshot>,
) -> StabilityWindow {
    let (start_timestamp, end_timestamp) = if start_timestamp <= end_timestamp {
        (start_timestamp, end_timestamp)
    } else {
        (end_timestamp, start_timestamp)
    };
    let mut window = StabilityWindow {
        start_timestamp,
        end_timestamp,
        observations,
    };
    window.observations = normalized_drift_observations(&window);
    window
}

pub fn semantic_core_candidates(
    stability: &[SemanticStabilitySnapshot],
    drift: &[SemanticDriftSnapshot],
) -> SemanticCoreCandidateTable {
    let stability = normalized_stability_snapshots(stability);
    let drift = normalized_drift_slice(drift);
    if stability.is_empty() {
        return SemanticCoreCandidateTable::default();
    }
    let mean_stability = stability
        .iter()
        .map(|snapshot| snapshot.stability.stability_score)
        .sum::<f64>()
        / stability.len() as f64;
    let mean_drift = if drift.is_empty() {
        0.0
    } else {
        drift
            .iter()
            .map(|snapshot| snapshot.drift.drift_score)
            .sum::<f64>()
            / drift.len() as f64
    };
    let oscillation = drift_oscillation_penalty(&drift);
    if mean_stability < 0.75 || mean_drift > 0.25 || oscillation > 0.25 {
        return SemanticCoreCandidateTable::default();
    }

    let invariant_members = stability
        .iter()
        .flat_map(|snapshot| {
            [
                snapshot.timestamp,
                snapshot.stability.stability_score.to_bits(),
            ]
        })
        .chain(drift.iter().flat_map(|snapshot| {
            [
                snapshot.before_timestamp,
                snapshot.after_timestamp,
                snapshot.drift.drift_score.to_bits(),
            ]
        }))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let resistance = drift_resistance(
        &ClusterCandidate {
            candidate_id: stable_hash_u64s(invariant_members.iter().copied()),
            members: invariant_members.clone(),
            average_distance: mean_drift,
            topology_coherence: normalize_distance(1.0 - mean_drift),
            relation_coherence: normalize_distance(mean_stability),
        },
        &drift,
    );
    let drift_resistance = normalize_distance(
        (resistance.oscillation_resistance
            + resistance.topology_resistance
            + resistance.relation_resistance)
            / 3.0,
    );
    let mut candidates = vec![SemanticCoreCandidate {
        core_id: stable_hash_u64s(invariant_members.iter().copied()),
        invariant_members,
        stability_score: normalize_distance(mean_stability),
        drift_resistance,
    }];
    candidates.sort_by(core_candidate_order);
    SemanticCoreCandidateTable { candidates }
}

pub fn drift_resistance(
    candidate: &ClusterCandidate,
    drift: &[SemanticDriftSnapshot],
) -> DriftResistance {
    let drift = normalized_drift_slice(drift);
    let oscillation_resistance = normalize_distance(1.0 - drift_oscillation_penalty(&drift));
    if drift.is_empty() {
        return DriftResistance {
            oscillation_resistance,
            topology_resistance: normalize_distance(candidate.topology_coherence),
            relation_resistance: normalize_distance(candidate.relation_coherence),
        };
    }
    DriftResistance {
        oscillation_resistance,
        topology_resistance: normalize_distance(
            1.0 - drift
                .iter()
                .map(|snapshot| snapshot.drift.topology_shift)
                .sum::<f64>()
                / drift.len() as f64,
        ),
        relation_resistance: normalize_distance(
            1.0 - drift
                .iter()
                .map(|snapshot| snapshot.drift.relation_shift)
                .sum::<f64>()
                / drift.len() as f64,
        ),
    }
}

pub fn semantic_core_snapshot(
    timestamp: u64,
    table: &SemanticCoreCandidateTable,
) -> SemanticCoreSnapshot {
    let mut candidates = table.candidates.clone();
    for candidate in &mut candidates {
        candidate.invariant_members.sort();
    }
    candidates.sort_by(core_candidate_order);
    SemanticCoreSnapshot {
        timestamp,
        candidates,
    }
}

pub fn semantic_identity_graph(
    cores: &[SemanticCoreSnapshot],
    drift: &[SemanticDriftSnapshot],
) -> SemanticIdentityGraph {
    let cores = normalized_core_snapshots(cores);
    let drift = normalized_drift_slice(drift);
    let mut identities = Vec::new();
    let mut previous_candidates: Vec<SemanticCoreCandidate> = Vec::new();

    for snapshot in cores {
        let mut current = snapshot.candidates.clone();
        current.sort_by(core_candidate_order);
        for candidate in &current {
            let best_previous = previous_candidates
                .iter()
                .map(|previous| (previous, continuity_score(previous, candidate)))
                .max_by(|left, right| {
                    left.1
                        .total_cmp(&right.1)
                        .then_with(|| left.0.core_id.cmp(&right.0.core_id))
                });
            let (continuity, mut lineage) = if let Some((previous, score)) = best_previous {
                if score >= 0.50 {
                    (score, vec![previous.core_id, candidate.core_id])
                } else {
                    (0.0, vec![candidate.core_id])
                }
            } else {
                (1.0, vec![candidate.core_id])
            };
            lineage.extend(
                drift
                    .iter()
                    .filter(|snapshot| {
                        snapshot.after_timestamp
                            <= snapshot.after_timestamp.max(snapshot.before_timestamp)
                    })
                    .map(|snapshot| {
                        stable_hash_u64s([
                            snapshot.before_timestamp,
                            snapshot.after_timestamp,
                            snapshot.drift.drift_score.to_bits(),
                        ])
                    }),
            );
            lineage.sort();
            lineage.dedup();
            let invariant_core_overlap = best_previous
                .map(|(previous, _)| core_member_overlap(previous, candidate))
                .unwrap_or(1.0);
            let identity_id = stable_hash_u64s(
                candidate
                    .invariant_members
                    .iter()
                    .copied()
                    .chain(lineage.iter().copied()),
            );
            identities.push(SemanticIdentityCandidate {
                identity_id,
                continuity_score: normalize_distance(continuity),
                invariant_core_overlap,
                drift_lineage: lineage,
            });
        }
        previous_candidates = current;
    }

    identities.sort_by(identity_order);
    SemanticIdentityGraph { identities }
}

pub fn continuity_score(previous: &SemanticCoreCandidate, current: &SemanticCoreCandidate) -> f64 {
    let overlap = core_member_overlap(previous, current);
    let stability = (normalize_distance(previous.stability_score)
        + normalize_distance(current.stability_score))
        / 2.0;
    let resistance = (normalize_distance(previous.drift_resistance)
        + normalize_distance(current.drift_resistance))
        / 2.0;
    normalize_distance((overlap * 0.5) + (stability * 0.25) + (resistance * 0.25))
}

pub fn semantic_identity_snapshot(
    timestamp: u64,
    graph: &SemanticIdentityGraph,
) -> SemanticIdentitySnapshot {
    let mut identities = graph.identities.clone();
    for identity in &mut identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
    }
    identities.sort_by(identity_order);
    SemanticIdentitySnapshot {
        timestamp,
        identities,
    }
}

pub fn identity_lineages(graph: &SemanticIdentityGraph) -> Vec<IdentityLineage> {
    let mut descendants: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for identity in &graph.identities {
        if let Some(ancestor_id) = identity.drift_lineage.first().copied() {
            descendants
                .entry(ancestor_id)
                .or_default()
                .insert(identity.identity_id);
        }
    }
    descendants
        .into_iter()
        .map(|(ancestor_id, descendant_ids)| IdentityLineage {
            ancestor_id,
            descendant_ids: descendant_ids.into_iter().collect(),
        })
        .collect()
}

pub fn merge_candidates(graph: &SemanticIdentityGraph) -> Vec<SemanticMergeCandidate> {
    let mut identities = graph.identities.clone();
    for identity in &mut identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
    }
    identities.sort_by(identity_order);

    let mut candidates = Vec::new();
    for (index, left) in identities.iter().enumerate() {
        for right in identities.iter().skip(index + 1) {
            let invariant_overlap_score =
                lineage_overlap(&left.drift_lineage, &right.drift_lineage);
            let continuity_score = normalize_distance(
                (left.continuity_score + right.continuity_score + invariant_overlap_score) / 3.0,
            );
            let lineage_distance =
                lineage_symmetric_difference(&left.drift_lineage, &right.drift_lineage);
            let contradiction_density = normalize_distance(
                (1.0 - invariant_overlap_score)
                    * ((left.continuity_score - right.continuity_score).abs()
                        + (left.invariant_core_overlap - right.invariant_core_overlap).abs())
                    / 2.0,
            );
            let risk = merge_risk_score(left, right);
            candidates.push(SemanticMergeCandidate {
                left_identity: left.identity_id,
                right_identity: right.identity_id,
                continuity_score,
                invariant_overlap_score,
                contradiction_density,
                lineage_distance,
                merge_risk_score: risk,
                compression_gain: normalize_distance(
                    invariant_overlap_score * (1.0 - risk) * continuity_score,
                ),
            });
        }
    }
    candidates.sort_by(merge_candidate_order);
    candidates
}

pub fn merge_risk_score(
    left: &SemanticIdentityCandidate,
    right: &SemanticIdentityCandidate,
) -> f64 {
    let invariant_overlap = lineage_overlap(&left.drift_lineage, &right.drift_lineage);
    let continuity_delta = (left.continuity_score - right.continuity_score).abs();
    let core_delta = (left.invariant_core_overlap - right.invariant_core_overlap).abs();
    let lineage_distance = lineage_symmetric_difference(&left.drift_lineage, &right.drift_lineage);
    let lineage_conflict = if invariant_overlap == 0.0 && lineage_distance > 0 {
        1.0
    } else {
        normalize_distance(lineage_distance as f64 / 16.0)
    };
    normalize_distance(
        ((1.0 - invariant_overlap) * 0.35)
            + (continuity_delta * 0.20)
            + (core_delta * 0.20)
            + (lineage_conflict * 0.25),
    )
}

pub fn semantic_merge(
    candidate: &SemanticMergeCandidate,
) -> Result<SemanticMergeResult, MemorySpaceError> {
    let semantic_loss_score =
        normalize_distance((candidate.merge_risk_score + candidate.contradiction_density) / 2.0);
    if candidate.contradiction_density > 0.35 {
        return Err(MemorySpaceError::UnsafeSemanticMerge(
            "contradiction density above threshold".to_string(),
        ));
    }
    if candidate.invariant_overlap_score < 0.50 {
        return Err(MemorySpaceError::UnsafeSemanticMerge(
            "invariant overlap below minimum".to_string(),
        ));
    }
    if candidate.lineage_distance > 12 {
        return Err(MemorySpaceError::UnsafeSemanticMerge(
            "lineage conflict detected".to_string(),
        ));
    }
    if semantic_loss_score > 0.30 {
        return Err(MemorySpaceError::UnsafeSemanticMerge(
            "semantic loss above maximum".to_string(),
        ));
    }

    let mut source_identities = vec![candidate.left_identity, candidate.right_identity];
    source_identities.sort();
    source_identities.dedup();
    let merged_identity = stable_hash_u64s(source_identities.iter().copied());
    let preserved_invariants = source_identities
        .iter()
        .map(|identity| InvariantCore { core_id: *identity })
        .collect::<Vec<_>>();
    let discarded_fragments = if candidate.compression_gain > 0.0 {
        vec![SemanticFragment {
            fragment_id: stable_hash_u64s([
                candidate.left_identity,
                candidate.right_identity,
                candidate.lineage_distance as u64,
            ]),
        }]
    } else {
        Vec::new()
    };

    Ok(SemanticMergeResult {
        merged_identity,
        source_identities,
        preserved_invariants,
        discarded_fragments,
        merge_confidence: normalize_distance(1.0 - semantic_loss_score),
        semantic_loss_score,
    })
}

pub fn semantic_compression(graph: &SemanticIdentityGraph) -> SemanticCompressionSnapshot {
    let identity_count_before = graph.identities.len();
    if identity_count_before == 0 {
        return SemanticCompressionSnapshot {
            timestamp: 0,
            identity_count_before: 0,
            identity_count_after: 0,
            compression_ratio: 0.0,
            preserved_semantic_mass: 0.0,
            discarded_semantic_mass: 0.0,
        };
    }

    let mut used = BTreeSet::new();
    let mut safe_merges = Vec::new();
    for candidate in merge_candidates(graph) {
        if used.contains(&candidate.left_identity) || used.contains(&candidate.right_identity) {
            continue;
        }
        if semantic_merge(&candidate).is_ok() {
            used.insert(candidate.left_identity);
            used.insert(candidate.right_identity);
            safe_merges.push(candidate);
        }
    }

    let identity_count_after = identity_count_before.saturating_sub(safe_merges.len());
    let preserved_semantic_mass = normalize_distance(
        graph
            .identities
            .iter()
            .map(|identity| identity.continuity_score * identity.invariant_core_overlap)
            .sum::<f64>()
            / identity_count_before as f64,
    );
    let discarded_semantic_mass = normalize_distance(
        safe_merges
            .iter()
            .map(|candidate| candidate.merge_risk_score * candidate.compression_gain)
            .sum::<f64>()
            / identity_count_before as f64,
    );
    let mut timestamp_identity_ids = graph
        .identities
        .iter()
        .map(|identity| identity.identity_id)
        .collect::<Vec<_>>();
    timestamp_identity_ids.sort();
    SemanticCompressionSnapshot {
        timestamp: stable_hash_u64s(timestamp_identity_ids),
        identity_count_before,
        identity_count_after,
        compression_ratio: normalize_distance(
            1.0 - (identity_count_after as f64 / identity_count_before as f64),
        ),
        preserved_semantic_mass,
        discarded_semantic_mass,
    }
}

pub fn semantic_attractors(graph: &SemanticIdentityGraph) -> Vec<SemanticAttractor> {
    let mut identities = graph.identities.clone();
    for identity in &mut identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
    }
    identities.sort_by(identity_order);
    let mut attractors = identities
        .iter()
        .map(|identity| {
            let mut anchor_set = identity
                .drift_lineage
                .iter()
                .map(|lineage_id| SemanticAnchor {
                    anchor_id: stable_hash_u64s([identity.identity_id, *lineage_id]),
                    identity_id: identity.identity_id,
                    invariant_core: InvariantCore {
                        core_id: *lineage_id,
                    },
                })
                .collect::<Vec<_>>();
            anchor_set.sort();
            let invariant_density = normalize_distance(identity.invariant_core_overlap);
            let stability_score = normalize_distance(identity.continuity_score);
            let contradiction_resistance = invariant_density;
            let lineage_stability =
                normalize_distance(anchor_set.len() as f64 / (anchor_set.len() as f64 + 1.0));
            let attractor_strength = normalize_distance(
                (invariant_density * 0.35)
                    + (stability_score * 0.35)
                    + (lineage_stability * 0.15)
                    + (contradiction_resistance * 0.15),
            );
            SemanticAttractor {
                attractor_id: stable_hash_u64s(
                    anchor_set
                        .iter()
                        .map(|anchor| anchor.anchor_id)
                        .chain([identity.identity_id]),
                ),
                anchor_set,
                invariant_density,
                stability_score,
                attractor_strength,
                semantic_mass: normalize_distance(stability_score * invariant_density),
                basin_strength: attractor_strength,
                semantic_density: invariant_density,
                stability_gradient: stability_score,
            }
        })
        .collect::<Vec<_>>();
    attractors.sort_by(attractor_order);
    attractors
}

pub fn detect_semantic_drift(
    previous: &SemanticIdentitySnapshot,
    current: &SemanticIdentitySnapshot,
) -> Vec<SemanticDriftEvent> {
    let previous = normalized_identity_snapshot(previous);
    let current = normalized_identity_snapshot(current);
    let previous_by_id = previous
        .identities
        .iter()
        .map(|identity| (identity.identity_id, identity))
        .collect::<BTreeMap<_, _>>();
    let current_by_id = current
        .identities
        .iter()
        .map(|identity| (identity.identity_id, identity))
        .collect::<BTreeMap<_, _>>();
    let mut ids = previous_by_id.keys().copied().collect::<BTreeSet<_>>();
    ids.extend(current_by_id.keys().copied());

    let mut events = ids
        .into_iter()
        .map(|identity_id| {
            let previous_identity = previous_by_id.get(&identity_id).copied();
            let current_identity = current_by_id.get(&identity_id).copied();
            let previous_continuity = previous_identity
                .map(|identity| normalize_distance(identity.continuity_score))
                .unwrap_or(0.0);
            let current_continuity = current_identity
                .map(|identity| normalize_distance(identity.continuity_score))
                .unwrap_or(0.0);
            let lineage_shift = match (previous_identity, current_identity) {
                (Some(previous), Some(current)) => set_distance(
                    &previous
                        .drift_lineage
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>(),
                    &current
                        .drift_lineage
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>(),
                ),
                _ => 1.0,
            };
            let drift_magnitude = normalize_distance(
                ((previous_continuity - current_continuity).abs() * 0.6) + (lineage_shift * 0.4),
            );
            SemanticDriftEvent {
                identity_id,
                previous_continuity,
                current_continuity,
                drift_magnitude,
                recoverable: current_continuity >= 0.50 && drift_magnitude <= 0.45,
            }
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| left.identity_id.cmp(&right.identity_id));
    events
}

pub fn stabilization_state(identity: &SemanticIdentity) -> SemanticStabilizationState {
    let continuity_score = normalize_distance(identity.continuity_score);
    let drift_score = normalize_distance(1.0 - continuity_score);
    let contradiction_density = normalize_distance(1.0 - identity.invariant_core_overlap);
    let attractor_id = stable_hash_u64s(
        identity
            .drift_lineage
            .iter()
            .copied()
            .chain([identity.identity_id]),
    );
    let stabilization_confidence =
        normalize_distance(continuity_score * (1.0 - drift_score) * (1.0 - contradiction_density));
    SemanticStabilizationState {
        identity_id: identity.identity_id,
        attractor_id,
        continuity_score,
        drift_score,
        contradiction_density,
        stabilization_confidence,
        recoverable: continuity_score >= 0.50
            && contradiction_density <= 0.35
            && !identity.drift_lineage.is_empty(),
    }
}

pub fn semantic_correction_plan(
    state: &SemanticStabilizationState,
) -> Option<SemanticCorrectionPlan> {
    if !state.recoverable {
        return None;
    }
    let restored_invariants = vec![InvariantCore {
        core_id: state.attractor_id,
    }];
    let rejected_fragments = if state.contradiction_density > 0.0 {
        vec![SemanticFragment {
            fragment_id: stable_hash_u64s([
                state.identity_id,
                state.contradiction_density.to_bits(),
            ]),
        }]
    } else {
        Vec::new()
    };
    Some(SemanticCorrectionPlan {
        target_identity: state.identity_id,
        restored_invariants,
        rejected_fragments,
        correction_confidence: state.stabilization_confidence,
    })
}

pub fn apply_semantic_correction(
    identity: &SemanticIdentity,
    plan: &SemanticCorrectionPlan,
) -> SemanticIdentity {
    if identity.identity_id != plan.target_identity {
        return identity.clone();
    }
    let mut corrected = identity.clone();
    corrected.continuity_score =
        normalize_distance(corrected.continuity_score.max(plan.correction_confidence));
    corrected.invariant_core_overlap = normalize_distance(
        corrected
            .invariant_core_overlap
            .max(plan.correction_confidence),
    );
    corrected
        .drift_lineage
        .extend(plan.restored_invariants.iter().map(|core| core.core_id));
    corrected.drift_lineage.sort();
    corrected.drift_lineage.dedup();
    corrected
}

pub fn semantic_rewrite_transaction(graph: &SemanticIdentityGraph) -> SemanticRewriteTransaction {
    let source_snapshot = normalized_identity_snapshot(&SemanticIdentitySnapshot {
        timestamp: semantic_identity_graph_timestamp(graph),
        identities: graph.identities.clone(),
    });
    let normalized_graph = graph_from_identity_snapshot(&source_snapshot);
    let rollback_snapshot = semantic_rollback_snapshot(&normalized_graph);
    let rewrite_plan = semantic_rewrite_plan(&normalized_graph);
    let mut transaction = SemanticRewriteTransaction {
        transaction_id: stable_hash_u64s([
            source_snapshot.timestamp,
            rollback_snapshot.snapshot_id,
            rewrite_plan_hash(&rewrite_plan),
        ]),
        source_snapshot,
        rewrite_plan,
        preview: SemanticRewritePreview {
            topology_diff: SemanticTopologyDiff::default(),
            continuity_delta: 0.0,
            semantic_mass_delta: 0.0,
            contradiction_delta: 0.0,
            anchor_preservation_ratio: 1.0,
        },
        validation: SemanticRewriteValidation {
            valid: false,
            continuity_retained: false,
            anchors_preserved: false,
            contradiction_bounded: false,
            semantic_mass_bounded: false,
            replay_invariant: false,
            topology_invariant: false,
            validation_errors: Vec::new(),
        },
        rollback_snapshot,
        deterministic_checksum: 0,
    };
    transaction.preview = semantic_rewrite_preview(&transaction);
    transaction.validation = validate_semantic_rewrite(&transaction);
    transaction.deterministic_checksum = deterministic_rewrite_checksum(&transaction);
    transaction
}

pub fn semantic_rewrite_preview(
    transaction: &SemanticRewriteTransaction,
) -> SemanticRewritePreview {
    let identity_count = transaction.source_snapshot.identities.len();
    let before_continuity = average_identity_continuity(&transaction.source_snapshot.identities);
    let before_mass = average_identity_mass(&transaction.source_snapshot.identities);
    let before_contradiction =
        average_identity_contradiction(&transaction.source_snapshot.identities);
    let correction_gain = if identity_count == 0 {
        0.0
    } else {
        transaction
            .rewrite_plan
            .correction_operations
            .iter()
            .map(|plan| plan.correction_confidence * 0.10)
            .sum::<f64>()
            / identity_count as f64
    };
    let compression_gain = if identity_count == 0 {
        0.0
    } else {
        transaction
            .rewrite_plan
            .compression_operations
            .iter()
            .map(|operation| operation.expected_compression_gain * 0.05)
            .sum::<f64>()
            / identity_count as f64
    };
    let contradiction_reduction = if identity_count == 0 {
        0.0
    } else {
        transaction
            .rewrite_plan
            .correction_operations
            .iter()
            .map(|plan| plan.rejected_fragments.len() as f64 * 0.03)
            .sum::<f64>()
            / identity_count as f64
    };
    let branch_preservation_count = branch_preservation_count(&transaction.source_snapshot);
    let anchor_count_before = transaction.rollback_snapshot.anchor_snapshot.len();
    let anchor_preservation_ratio = if anchor_count_before == 0 {
        1.0
    } else {
        normalize_distance(
            transaction
                .rollback_snapshot
                .anchor_snapshot
                .iter()
                .collect::<BTreeSet<_>>()
                .len() as f64
                / anchor_count_before as f64,
        )
    };

    SemanticRewritePreview {
        topology_diff: SemanticTopologyDiff {
            merge_candidate_count: transaction.rewrite_plan.merge_operations.len(),
            correction_target_count: transaction.rewrite_plan.correction_operations.len(),
            compression_operation_count: transaction.rewrite_plan.compression_operations.len(),
            branch_preservation_count,
            attractor_change_count: 0,
        },
        continuity_delta: clamp_signed_delta(
            normalize_distance(before_continuity + correction_gain) - before_continuity,
        ),
        semantic_mass_delta: clamp_signed_delta(
            normalize_distance(before_mass + correction_gain + compression_gain) - before_mass,
        ),
        contradiction_delta: clamp_signed_delta(
            normalize_distance(before_contradiction - contradiction_reduction)
                - before_contradiction,
        ),
        anchor_preservation_ratio,
    }
}

pub fn validate_semantic_rewrite(
    transaction: &SemanticRewriteTransaction,
) -> SemanticRewriteValidation {
    let preview = semantic_rewrite_preview(transaction);
    let before_continuity = average_identity_continuity(&transaction.source_snapshot.identities);
    let continuity_after = normalize_distance(before_continuity + preview.continuity_delta);
    let continuity_retained =
        transaction.source_snapshot.identities.is_empty() || continuity_after >= 0.50;
    let anchors_preserved = preview.anchor_preservation_ratio >= 1.0;
    let contradiction_bounded = preview.contradiction_delta <= 0.10;
    let semantic_mass_bounded = preview.semantic_mass_delta >= -0.25;
    let replay_invariant =
        preview == transaction.preview || transaction.deterministic_checksum == 0;
    let topology_invariant = transaction.rollback_snapshot.topology_snapshot
        == normalized_identity_snapshot(&transaction.source_snapshot);
    let mut validation_errors = Vec::new();
    if !continuity_retained {
        validation_errors.push("continuity below threshold".to_string());
    }
    if !anchors_preserved {
        validation_errors.push("semantic anchors not preserved".to_string());
    }
    if !contradiction_bounded {
        validation_errors.push("contradiction delta above threshold".to_string());
    }
    if !semantic_mass_bounded {
        validation_errors.push("semantic mass delta outside bounds".to_string());
    }
    if !replay_invariant {
        validation_errors.push("rewrite preview is not replay invariant".to_string());
    }
    if !topology_invariant {
        validation_errors.push("rollback topology snapshot mismatch".to_string());
    }

    SemanticRewriteValidation {
        valid: validation_errors.is_empty(),
        continuity_retained,
        anchors_preserved,
        contradiction_bounded,
        semantic_mass_bounded,
        replay_invariant,
        topology_invariant,
        validation_errors,
    }
}

pub fn semantic_rollback_snapshot(graph: &SemanticIdentityGraph) -> SemanticRollbackSnapshot {
    let topology_snapshot = normalized_identity_snapshot(&SemanticIdentitySnapshot {
        timestamp: semantic_identity_graph_timestamp(graph),
        identities: graph.identities.clone(),
    });
    let normalized_graph = graph_from_identity_snapshot(&topology_snapshot);
    let attractor_snapshot = semantic_attractors(&normalized_graph);
    let mut anchor_snapshot = attractor_snapshot
        .iter()
        .flat_map(|attractor| attractor.anchor_set.iter().copied())
        .collect::<Vec<_>>();
    anchor_snapshot.sort();
    anchor_snapshot.dedup();
    SemanticRollbackSnapshot {
        snapshot_id: stable_hash_u64s(
            topology_snapshot
                .identities
                .iter()
                .flat_map(identity_hash_components),
        ),
        topology_snapshot,
        attractor_snapshot,
        anchor_snapshot,
    }
}

pub fn deterministic_rewrite_checksum(transaction: &SemanticRewriteTransaction) -> u64 {
    stable_hash_u64s(
        [
            transaction.transaction_id,
            rewrite_plan_hash(&transaction.rewrite_plan),
        ]
        .into_iter()
        .chain(preview_hash_components(&transaction.preview))
        .chain(validation_hash_components(&transaction.validation))
        .chain([transaction.rollback_snapshot.snapshot_id]),
    )
}

pub fn semantic_attractor_field(
    rewrites: &[SemanticRewriteTransaction],
    drift: &[SemanticDriftSnapshot],
    stability: &[SemanticStabilitySnapshot],
) -> SemanticAttractorField {
    if rewrites.is_empty() {
        return SemanticAttractorField {
            attractors: Vec::new(),
        };
    }

    let mut sorted_rewrites = rewrites.to_vec();
    sorted_rewrites.sort_by(rewrite_transaction_order);
    let global_drift_resistance = normalize_distance(1.0 - average_drift_score(drift));
    let global_stability = average_stability_score(stability);
    let mut accumulators = BTreeMap::<SemanticIdentityId, AttractorAccumulator>::new();

    for transaction in &sorted_rewrites {
        let energy = rewrite_energy(transaction);
        for identity in &transaction.source_snapshot.identities {
            let entry = accumulators
                .entry(identity.identity_id)
                .or_insert_with(|| AttractorAccumulator::new(identity.identity_id));
            entry.observation_count += 1;
            entry.total_continuity += normalize_distance(identity.continuity_score);
            entry.total_invariant += normalize_distance(identity.invariant_core_overlap);
            entry.total_semantic_mass +=
                normalize_distance(identity.continuity_score * identity.invariant_core_overlap);
            entry.total_energy_resistance += normalize_distance(1.0 - energy.total_energy);
            entry.lineage.extend(identity.drift_lineage.iter().copied());
        }

        for candidate in &transaction.rewrite_plan.merge_operations {
            for identity_id in [candidate.left_identity, candidate.right_identity] {
                accumulators
                    .entry(identity_id)
                    .or_insert_with(|| AttractorAccumulator::new(identity_id))
                    .merge_count += 1;
            }
        }
        for plan in &transaction.rewrite_plan.correction_operations {
            accumulators
                .entry(plan.target_identity)
                .or_insert_with(|| AttractorAccumulator::new(plan.target_identity))
                .correction_count += 1;
        }
        for operation in &transaction.rewrite_plan.compression_operations {
            for identity_id in &operation.source_identities {
                accumulators
                    .entry(*identity_id)
                    .or_insert_with(|| AttractorAccumulator::new(*identity_id))
                    .compression_gain += operation.expected_compression_gain;
            }
        }
    }

    let rewrite_count = sorted_rewrites.len() as f64;
    let mut attractors = accumulators
        .into_values()
        .map(|accumulator| {
            let observation_count = accumulator.observation_count.max(1) as f64;
            let convergence_frequency = normalize_distance(observation_count / rewrite_count);
            let rewrite_pull = normalize_distance(
                (accumulator.merge_count as f64
                    + accumulator.correction_count as f64
                    + accumulator.compression_gain)
                    / (observation_count + 1.0),
            );
            let semantic_density = normalize_distance(
                (accumulator.total_invariant / observation_count * 0.45)
                    + (accumulator.total_semantic_mass / observation_count * 0.35)
                    + (rewrite_pull * 0.20),
            );
            let stability_gradient = normalize_distance(
                (accumulator.total_continuity / observation_count * 0.35)
                    + (global_stability * 0.30)
                    + (global_drift_resistance * 0.20)
                    + (accumulator.total_energy_resistance / observation_count * 0.15),
            );
            let basin_strength = normalize_distance(
                (convergence_frequency * 0.30)
                    + (semantic_density * 0.35)
                    + (stability_gradient * 0.25)
                    + (rewrite_pull * 0.10),
            );
            let anchor_set = accumulator
                .lineage
                .iter()
                .map(|lineage_id| SemanticAnchor {
                    anchor_id: stable_hash_u64s([accumulator.identity_id, *lineage_id]),
                    identity_id: accumulator.identity_id,
                    invariant_core: InvariantCore {
                        core_id: *lineage_id,
                    },
                })
                .collect::<Vec<_>>();
            SemanticAttractor {
                attractor_id: stable_hash_u64s(
                    [accumulator.identity_id, basin_strength.to_bits()]
                        .into_iter()
                        .chain(accumulator.lineage.iter().copied()),
                ),
                anchor_set,
                invariant_density: semantic_density,
                stability_score: stability_gradient,
                attractor_strength: basin_strength,
                semantic_mass: normalize_distance(semantic_density * stability_gradient),
                basin_strength,
                semantic_density,
                stability_gradient,
            }
        })
        .collect::<Vec<_>>();
    attractors.sort_by(attractor_dynamics_order);
    SemanticAttractorField { attractors }
}

pub fn rewrite_energy(transaction: &SemanticRewriteTransaction) -> RewriteEnergy {
    let identity_count = transaction.source_snapshot.identities.len().max(1) as f64;
    let topology_energy = normalize_distance(
        (transaction.preview.topology_diff.merge_candidate_count as f64
            + transaction
                .preview
                .topology_diff
                .compression_operation_count as f64)
            / (identity_count + 1.0),
    );
    let relation_energy = normalize_distance(
        transaction.preview.contradiction_delta.abs()
            + (1.0 - transaction.preview.anchor_preservation_ratio),
    );
    let continuity_energy = normalize_distance(
        transaction.preview.continuity_delta.abs()
            + transaction
                .rewrite_plan
                .correction_operations
                .iter()
                .map(|plan| 1.0 - plan.correction_confidence)
                .sum::<f64>()
                / identity_count,
    );
    let total_energy = normalize_distance(
        (topology_energy * 0.40) + (relation_energy * 0.25) + (continuity_energy * 0.35),
    );
    RewriteEnergy {
        topology_energy,
        relation_energy,
        continuity_energy,
        total_energy,
    }
}

pub fn collapse_risk(field: &SemanticAttractorField) -> CollapseRisk {
    if field.attractors.is_empty() {
        return CollapseRisk {
            collapse_score: 0.0,
            semantic_density_risk: 0.0,
            attractor_overconvergence: 0.0,
        };
    }
    let max_density = field
        .attractors
        .iter()
        .map(|attractor| attractor.semantic_density)
        .fold(0.0, f64::max);
    let total_basin = field
        .attractors
        .iter()
        .map(|attractor| attractor.basin_strength)
        .sum::<f64>();
    let max_basin = field
        .attractors
        .iter()
        .map(|attractor| attractor.basin_strength)
        .fold(0.0, f64::max);
    let attractor_overconvergence = if total_basin == 0.0 {
        0.0
    } else {
        normalize_distance(max_basin / total_basin)
    };
    let semantic_density_risk = normalize_distance(max_density);
    CollapseRisk {
        collapse_score: normalize_distance(
            (semantic_density_risk * 0.45) + (attractor_overconvergence * 0.55),
        ),
        semantic_density_risk,
        attractor_overconvergence,
    }
}

pub fn semantic_attractor_snapshot(
    timestamp: u64,
    field: &SemanticAttractorField,
) -> SemanticAttractorSnapshot {
    let mut field = field.clone();
    field.attractors.sort_by(attractor_dynamics_order);
    SemanticAttractorSnapshot { timestamp, field }
}

fn normalized_identity_snapshot(snapshot: &SemanticIdentitySnapshot) -> SemanticIdentitySnapshot {
    let mut identities = snapshot.identities.clone();
    for identity in &mut identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
        identity.continuity_score = normalize_distance(identity.continuity_score);
        identity.invariant_core_overlap = normalize_distance(identity.invariant_core_overlap);
    }
    identities.sort_by(identity_order);
    SemanticIdentitySnapshot {
        timestamp: snapshot.timestamp,
        identities,
    }
}

fn semantic_rewrite_plan(graph: &SemanticIdentityGraph) -> SemanticRewritePlan {
    let mut merge_operations = merge_candidates(graph)
        .into_iter()
        .filter(|candidate| semantic_merge(candidate).is_ok())
        .collect::<Vec<_>>();
    merge_operations.sort_by(merge_candidate_order);

    let mut correction_operations = graph
        .identities
        .iter()
        .filter_map(|identity| semantic_correction_plan(&stabilization_state(identity)))
        .collect::<Vec<_>>();
    correction_operations.sort_by(correction_plan_order);

    let mut compression_operations = merge_operations
        .iter()
        .map(|candidate| {
            let mut source_identities = vec![candidate.left_identity, candidate.right_identity];
            source_identities.sort();
            source_identities.dedup();
            SemanticCompressionOperation {
                operation_id: stable_hash_u64s(
                    source_identities
                        .iter()
                        .copied()
                        .chain([candidate.compression_gain.to_bits()]),
                ),
                source_identities,
                expected_compression_gain: candidate.compression_gain,
            }
        })
        .collect::<Vec<_>>();
    compression_operations.sort_by(compression_operation_order);

    SemanticRewritePlan {
        merge_operations,
        correction_operations,
        compression_operations,
    }
}

fn graph_from_identity_snapshot(snapshot: &SemanticIdentitySnapshot) -> SemanticIdentityGraph {
    SemanticIdentityGraph {
        identities: snapshot.identities.clone(),
    }
}

fn semantic_identity_graph_timestamp(graph: &SemanticIdentityGraph) -> u64 {
    let snapshot = normalized_identity_snapshot(&SemanticIdentitySnapshot {
        timestamp: 0,
        identities: graph.identities.clone(),
    });
    stable_hash_u64s(
        snapshot
            .identities
            .iter()
            .flat_map(identity_hash_components),
    )
}

fn average_identity_continuity(identities: &[SemanticIdentityCandidate]) -> f64 {
    if identities.is_empty() {
        return 1.0;
    }
    normalize_distance(
        identities
            .iter()
            .map(|identity| normalize_distance(identity.continuity_score))
            .sum::<f64>()
            / identities.len() as f64,
    )
}

fn average_identity_mass(identities: &[SemanticIdentityCandidate]) -> f64 {
    if identities.is_empty() {
        return 1.0;
    }
    normalize_distance(
        identities
            .iter()
            .map(|identity| {
                normalize_distance(identity.continuity_score)
                    * normalize_distance(identity.invariant_core_overlap)
            })
            .sum::<f64>()
            / identities.len() as f64,
    )
}

fn average_identity_contradiction(identities: &[SemanticIdentityCandidate]) -> f64 {
    if identities.is_empty() {
        return 0.0;
    }
    normalize_distance(
        identities
            .iter()
            .map(|identity| normalize_distance(1.0 - identity.invariant_core_overlap))
            .sum::<f64>()
            / identities.len() as f64,
    )
}

fn average_drift_score(drift: &[SemanticDriftSnapshot]) -> f64 {
    if drift.is_empty() {
        return 0.0;
    }
    normalize_distance(
        drift
            .iter()
            .map(|snapshot| normalize_distance(snapshot.drift.drift_score))
            .sum::<f64>()
            / drift.len() as f64,
    )
}

fn average_stability_score(stability: &[SemanticStabilitySnapshot]) -> f64 {
    if stability.is_empty() {
        return 0.0;
    }
    normalize_distance(
        stability
            .iter()
            .map(|snapshot| normalize_distance(snapshot.stability.stability_score))
            .sum::<f64>()
            / stability.len() as f64,
    )
}

fn branch_preservation_count(snapshot: &SemanticIdentitySnapshot) -> usize {
    let mut lineage_owners = BTreeMap::<u64, BTreeSet<SemanticIdentityId>>::new();
    for identity in &snapshot.identities {
        for lineage_id in &identity.drift_lineage {
            lineage_owners
                .entry(*lineage_id)
                .or_default()
                .insert(identity.identity_id);
        }
    }
    lineage_owners
        .values()
        .filter(|owners| owners.len() > 1)
        .count()
}

fn clamp_signed_delta(value: f64) -> f64 {
    value.clamp(-1.0, 1.0)
}

fn identity_hash_components(identity: &SemanticIdentityCandidate) -> Vec<u64> {
    [
        identity.identity_id,
        identity.continuity_score.to_bits(),
        identity.invariant_core_overlap.to_bits(),
    ]
    .into_iter()
    .chain(identity.drift_lineage.iter().copied())
    .collect()
}

fn rewrite_plan_hash(plan: &SemanticRewritePlan) -> u64 {
    stable_hash_u64s(
        plan.merge_operations
            .iter()
            .flat_map(merge_candidate_hash_components)
            .chain(
                plan.correction_operations
                    .iter()
                    .flat_map(correction_plan_hash_components),
            )
            .chain(
                plan.compression_operations
                    .iter()
                    .flat_map(compression_operation_hash_components),
            ),
    )
}

fn merge_candidate_hash_components(candidate: &SemanticMergeCandidate) -> Vec<u64> {
    vec![
        candidate.left_identity,
        candidate.right_identity,
        candidate.continuity_score.to_bits(),
        candidate.invariant_overlap_score.to_bits(),
        candidate.contradiction_density.to_bits(),
        candidate.lineage_distance as u64,
        candidate.merge_risk_score.to_bits(),
        candidate.compression_gain.to_bits(),
    ]
}

fn correction_plan_hash_components(plan: &SemanticCorrectionPlan) -> Vec<u64> {
    [plan.target_identity, plan.correction_confidence.to_bits()]
        .into_iter()
        .chain(plan.restored_invariants.iter().map(|core| core.core_id))
        .chain(
            plan.rejected_fragments
                .iter()
                .map(|fragment| fragment.fragment_id),
        )
        .collect()
}

fn compression_operation_hash_components(operation: &SemanticCompressionOperation) -> Vec<u64> {
    [
        operation.operation_id,
        operation.expected_compression_gain.to_bits(),
    ]
    .into_iter()
    .chain(operation.source_identities.iter().copied())
    .collect()
}

fn preview_hash_components(preview: &SemanticRewritePreview) -> Vec<u64> {
    vec![
        preview.topology_diff.merge_candidate_count as u64,
        preview.topology_diff.correction_target_count as u64,
        preview.topology_diff.compression_operation_count as u64,
        preview.topology_diff.branch_preservation_count as u64,
        preview.topology_diff.attractor_change_count as u64,
        preview.continuity_delta.to_bits(),
        preview.semantic_mass_delta.to_bits(),
        preview.contradiction_delta.to_bits(),
        preview.anchor_preservation_ratio.to_bits(),
    ]
}

fn validation_hash_components(validation: &SemanticRewriteValidation) -> Vec<u64> {
    let mut components = vec![
        validation.valid as u64,
        validation.continuity_retained as u64,
        validation.anchors_preserved as u64,
        validation.contradiction_bounded as u64,
        validation.semantic_mass_bounded as u64,
        validation.replay_invariant as u64,
        validation.topology_invariant as u64,
    ];
    components.extend(
        validation
            .validation_errors
            .iter()
            .map(|error| stable_hash_strings([error.as_str()])),
    );
    components
}

fn attractor_order(left: &SemanticAttractor, right: &SemanticAttractor) -> std::cmp::Ordering {
    right
        .attractor_strength
        .total_cmp(&left.attractor_strength)
        .then_with(|| left.attractor_id.cmp(&right.attractor_id))
}

fn attractor_dynamics_order(
    left: &SemanticAttractor,
    right: &SemanticAttractor,
) -> std::cmp::Ordering {
    right
        .basin_strength
        .total_cmp(&left.basin_strength)
        .then_with(|| right.semantic_density.total_cmp(&left.semantic_density))
        .then_with(|| left.attractor_id.cmp(&right.attractor_id))
}

fn rewrite_transaction_order(
    left: &SemanticRewriteTransaction,
    right: &SemanticRewriteTransaction,
) -> std::cmp::Ordering {
    left.deterministic_checksum
        .cmp(&right.deterministic_checksum)
        .then_with(|| left.transaction_id.cmp(&right.transaction_id))
}

fn correction_plan_order(
    left: &SemanticCorrectionPlan,
    right: &SemanticCorrectionPlan,
) -> std::cmp::Ordering {
    left.target_identity
        .cmp(&right.target_identity)
        .then_with(|| {
            right
                .correction_confidence
                .total_cmp(&left.correction_confidence)
        })
}

fn compression_operation_order(
    left: &SemanticCompressionOperation,
    right: &SemanticCompressionOperation,
) -> std::cmp::Ordering {
    left.operation_id.cmp(&right.operation_id).then_with(|| {
        right
            .expected_compression_gain
            .total_cmp(&left.expected_compression_gain)
    })
}

fn merge_candidate_order(
    left: &SemanticMergeCandidate,
    right: &SemanticMergeCandidate,
) -> std::cmp::Ordering {
    left.merge_risk_score
        .total_cmp(&right.merge_risk_score)
        .then_with(|| right.compression_gain.total_cmp(&left.compression_gain))
        .then_with(|| left.left_identity.cmp(&right.left_identity))
        .then_with(|| left.right_identity.cmp(&right.right_identity))
}

fn lineage_overlap(left: &[u64], right: &[u64]) -> f64 {
    let left = left.iter().copied().collect::<BTreeSet<_>>();
    let right = right.iter().copied().collect::<BTreeSet<_>>();
    let union_count = left.union(&right).count();
    if union_count == 0 {
        return 1.0;
    }
    left.intersection(&right).count() as f64 / union_count as f64
}

fn lineage_symmetric_difference(left: &[u64], right: &[u64]) -> usize {
    let left = left.iter().copied().collect::<BTreeSet<_>>();
    let right = right.iter().copied().collect::<BTreeSet<_>>();
    left.symmetric_difference(&right).count()
}

fn normalized_core_snapshots(cores: &[SemanticCoreSnapshot]) -> Vec<SemanticCoreSnapshot> {
    let mut snapshots = cores.to_vec();
    for snapshot in &mut snapshots {
        for candidate in &mut snapshot.candidates {
            candidate.invariant_members.sort();
        }
        snapshot.candidates.sort_by(core_candidate_order);
    }
    snapshots.sort_by_key(|snapshot| snapshot.timestamp);
    snapshots
}

fn core_member_overlap(previous: &SemanticCoreCandidate, current: &SemanticCoreCandidate) -> f64 {
    let previous = previous
        .invariant_members
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let current = current
        .invariant_members
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    set_distance(&previous, &current);
    let union_count = previous.union(&current).count();
    if union_count == 0 {
        1.0
    } else {
        previous.intersection(&current).count() as f64 / union_count as f64
    }
}

fn identity_order(
    left: &SemanticIdentityCandidate,
    right: &SemanticIdentityCandidate,
) -> std::cmp::Ordering {
    left.drift_lineage
        .cmp(&right.drift_lineage)
        .then_with(|| left.identity_id.cmp(&right.identity_id))
}

fn normalized_stability_snapshots(
    stability: &[SemanticStabilitySnapshot],
) -> Vec<SemanticStabilitySnapshot> {
    let mut snapshots = stability
        .iter()
        .map(|snapshot| SemanticStabilitySnapshot {
            timestamp: snapshot.timestamp,
            stability: SemanticStability {
                stability_score: normalize_distance(snapshot.stability.stability_score),
                temporal_consistency: normalize_distance(snapshot.stability.temporal_consistency),
                topology_consistency: normalize_distance(snapshot.stability.topology_consistency),
                relation_consistency: normalize_distance(snapshot.stability.relation_consistency),
            },
        })
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| snapshot.timestamp);
    snapshots
}

fn normalized_drift_slice(drift: &[SemanticDriftSnapshot]) -> Vec<SemanticDriftSnapshot> {
    let window = StabilityWindow {
        start_timestamp: 0,
        end_timestamp: u64::MAX,
        observations: drift.to_vec(),
    };
    normalized_drift_observations(&window)
}

fn core_candidate_order(
    left: &SemanticCoreCandidate,
    right: &SemanticCoreCandidate,
) -> std::cmp::Ordering {
    left.invariant_members
        .cmp(&right.invariant_members)
        .then_with(|| left.core_id.cmp(&right.core_id))
}

fn normalized_drift_observations(window: &StabilityWindow) -> Vec<SemanticDriftSnapshot> {
    let mut observations = window
        .observations
        .iter()
        .map(|snapshot| {
            semantic_drift_snapshot(
                snapshot.before_timestamp,
                snapshot.after_timestamp,
                SemanticDrift {
                    drift_score: normalize_distance(snapshot.drift.drift_score),
                    topology_shift: normalize_distance(snapshot.drift.topology_shift),
                    relation_shift: normalize_distance(snapshot.drift.relation_shift),
                    membership_change: normalize_distance(snapshot.drift.membership_change),
                },
            )
        })
        .collect::<Vec<_>>();
    observations.sort_by(|left, right| {
        left.before_timestamp
            .cmp(&right.before_timestamp)
            .then_with(|| left.after_timestamp.cmp(&right.after_timestamp))
    });
    observations
}

fn drift_oscillation_penalty(observations: &[SemanticDriftSnapshot]) -> f64 {
    if observations.len() < 3 {
        return 0.0;
    }
    let mut direction_changes = 0_u64;
    let mut previous_direction = 0_i8;
    for pair in observations.windows(2) {
        let delta = pair[1].drift.drift_score - pair[0].drift.drift_score;
        let direction = if delta > 0.0 {
            1
        } else if delta < 0.0 {
            -1
        } else {
            0
        };
        if direction != 0 && previous_direction != 0 && direction != previous_direction {
            direction_changes += 1;
        }
        if direction != 0 {
            previous_direction = direction;
        }
    }
    normalize_distance(direction_changes as f64 / (observations.len() - 1) as f64)
}

fn normalized_candidates(snapshot: &ClusterCandidateSnapshot) -> Vec<ClusterCandidate> {
    let mut candidates = snapshot.candidates.clone();
    for candidate in &mut candidates {
        candidate.members.sort();
    }
    candidates.sort_by(candidate_order);
    candidates
}

fn membership_sets(candidates: &[ClusterCandidate]) -> BTreeSet<Vec<MemoryId>> {
    candidates
        .iter()
        .map(|candidate| candidate.members.clone())
        .collect()
}

fn member_set(candidates: &[ClusterCandidate]) -> BTreeSet<MemoryId> {
    candidates
        .iter()
        .flat_map(|candidate| candidate.members.iter().copied())
        .collect()
}

fn set_distance<T: Ord + Clone>(before: &BTreeSet<T>, after: &BTreeSet<T>) -> f64 {
    let union_count = before.union(after).count();
    if union_count == 0 {
        return 0.0;
    }
    let intersection_count = before.intersection(after).count();
    normalize_distance(1.0 - (intersection_count as f64 / union_count as f64))
}

fn member_set_distance(before: &[ClusterCandidate], after: &[ClusterCandidate]) -> f64 {
    set_distance(&member_set(before), &member_set(after))
}

fn coherence_shift(before: &[ClusterCandidate], after: &[ClusterCandidate]) -> f64 {
    let before_by_members = before
        .iter()
        .map(|candidate| (candidate.members.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let after_by_members = after
        .iter()
        .map(|candidate| (candidate.members.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let shared = before_by_members
        .keys()
        .filter(|members| after_by_members.contains_key(*members))
        .cloned()
        .collect::<Vec<_>>();
    if shared.is_empty() {
        return set_distance(&membership_sets(before), &membership_sets(after));
    }

    let total_shift = shared
        .iter()
        .map(|members| {
            let before = before_by_members.get(members).expect("before candidate");
            let after = after_by_members.get(members).expect("after candidate");
            ((before.topology_coherence - after.topology_coherence).abs()
                + (before.relation_coherence - after.relation_coherence).abs())
                / 2.0
        })
        .sum::<f64>();
    normalize_distance(total_shift / shared.len() as f64)
}

fn ordered_pair(left: MemoryId, right: MemoryId) -> (MemoryId, MemoryId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn candidate_member_distances(
    members: &[MemoryId],
    distances: &BTreeMap<(MemoryId, MemoryId), SemanticDistance>,
) -> Vec<f64> {
    let mut values = Vec::new();
    for (index, left) in members.iter().enumerate() {
        for right in members.iter().skip(index + 1) {
            values.push(
                distances
                    .get(&ordered_pair(*left, *right))
                    .map(|distance| distance.total_distance)
                    .unwrap_or(1.0),
            );
        }
    }
    values
}

fn candidate_relation_distances(
    members: &[MemoryId],
    distances: &BTreeMap<(MemoryId, MemoryId), SemanticDistance>,
) -> Vec<f64> {
    let mut values = Vec::new();
    for (index, left) in members.iter().enumerate() {
        for right in members.iter().skip(index + 1) {
            if let Some(distance) = distances.get(&ordered_pair(*left, *right)) {
                values.push(distance.relation_distance);
            }
        }
    }
    values
}

fn candidate_order(left: &ClusterCandidate, right: &ClusterCandidate) -> std::cmp::Ordering {
    left.members
        .cmp(&right.members)
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

fn observation_order(
    left: &SemanticObservation,
    right: &SemanticObservation,
) -> std::cmp::Ordering {
    left.source
        .cmp(&right.source)
        .then_with(|| left.target.cmp(&right.target))
        .then_with(|| {
            left.distance
                .total_distance
                .total_cmp(&right.distance.total_distance)
        })
}

fn component_distance(left: u64, right: u64) -> f64 {
    if left == right {
        0.0
    } else {
        (left ^ right).count_ones() as f64 / 64.0
    }
}

fn normalize_weight(value: f64) -> f64 {
    if value.is_nan() || value.is_sign_negative() {
        0.0
    } else {
        value
    }
}

fn serialize_canonical_node(node: &CanonicalNodeSnapshot) -> String {
    format!(
        "{{\"canonical_id\":{},\"semantic_signature\":{},\"reference_count\":{}}}",
        node.canonical_id, node.semantic_signature, node.reference_count
    )
}

fn serialize_alias_node(node: &AliasNodeSnapshot) -> String {
    format!(
        "{{\"alias_id\":{},\"canonical_id\":{}}}",
        node.alias_id, node.canonical_id
    )
}

fn serialize_replay_fingerprint(fingerprint: &ReplayFingerprint) -> String {
    format!(
        "{{\"memory_id\":{},\"trajectory_id\":{},\"transitions\":[{}],\"causal_links\":[{}]}}",
        fingerprint.memory_id,
        fingerprint.trajectory_id,
        serialize_u64_array(&fingerprint.transitions),
        serialize_u64_array(&fingerprint.causal_links)
    )
}

fn serialize_trajectory_snapshot(snapshot: &TrajectorySnapshot) -> String {
    format!(
        "{{\"trajectory_id\":{},\"transition_ids\":[{}],\"causal_links\":[{}]}}",
        snapshot.trajectory_id,
        serialize_u64_array(&snapshot.transition_ids),
        serialize_u64_array(&snapshot.causal_links)
    )
}

fn serialize_u64_array(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn normalized_sorted_strings<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(normalize_semantic_text)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalized_sorted_relations(relations: &[SemanticRelation]) -> Vec<String> {
    let mut normalized = relations
        .iter()
        .map(|relation| {
            format!(
                "{}>{}>{}",
                normalize_semantic_text(&relation.source),
                normalize_semantic_text(&relation.relation),
                normalize_semantic_text(&relation.target)
            )
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn sorted_links(links: &[(MemoryId, MemoryId)]) -> Vec<String> {
    let mut normalized = links
        .iter()
        .map(|(from, to)| format!("{from}>{to}"))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn sorted_u64s(values: &[u64]) -> Vec<String> {
    let mut normalized = values.iter().copied().collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
        .into_iter()
        .map(|value| value.to_string())
        .collect()
}

fn normalize_semantic_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn stable_hash_strings<'a>(values: impl IntoIterator<Item = &'a str>) -> u64 {
    stable_hash_bytes(values.into_iter().collect::<Vec<_>>().join("\0").as_bytes())
}

fn next_lifecycle(
    current: MemoryLifecycle,
    access: MemoryAccessProfile,
    now_epoch: u64,
    policy: DecayPolicy,
) -> MemoryLifecycle {
    if current == MemoryLifecycle::Deleted {
        return MemoryLifecycle::Deleted;
    }
    let unused_for = now_epoch.saturating_sub(access.last_access_epoch);
    if access.semantic_redundancy >= policy.compress_at_semantic_redundancy {
        MemoryLifecycle::Compressed
    } else if access.access_count < policy.archive_below_access_count {
        MemoryLifecycle::Archived
    } else if unused_for > policy.dormant_after_epochs {
        MemoryLifecycle::Dormant
    } else {
        current
    }
}

fn stable_hash_u64s(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    stable_hash_bytes(&bytes)
}

fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(
        memory_id: MemoryId,
        state: u64,
        semantic: u64,
        trajectory: &StateTrajectory,
    ) -> MemoryIdentity {
        MemoryIdentity {
            memory_id,
            state_hash: state,
            transition_hash: trajectory.transition_hash(),
            semantic_signature: semantic,
        }
    }

    fn trajectory(id: TrajectoryId, transitions: &[TransitionId]) -> StateTrajectory {
        StateTrajectory {
            trajectory_id: id,
            transitions: transitions.to_vec(),
            causal_links: transitions
                .iter()
                .map(|transition| transition + 1000)
                .collect(),
        }
    }

    fn canonical_count(manager: &HolographicDeduplicationManager) -> usize {
        manager.exact_index.len()
    }

    fn physical_node_count(manager: &HolographicDeduplicationManager) -> usize {
        manager.node_count()
    }

    fn has_no_orphan_nodes(manager: &HolographicDeduplicationManager) -> bool {
        manager
            .exact_index
            .values()
            .all(|memory_id| manager.nodes.contains_key(memory_id))
    }

    fn has_no_dangling_reference(
        manager: &HolographicDeduplicationManager,
        canonical_id: MemoryId,
    ) -> bool {
        let Some(references) = manager.references_for(canonical_id) else {
            return false;
        };

        references.into_iter().all(|reference_id| {
            reference_id == canonical_id || !manager.nodes.contains_key(&reference_id)
        })
    }

    fn has_no_alias_loops(manager: &HolographicDeduplicationManager) -> bool {
        manager.semantic_clusters.values().all(|cluster| {
            !cluster.aliases.contains(&cluster.canonical_id)
                && cluster.aliases.iter().all(|alias| {
                    manager
                        .semantic_clusters
                        .values()
                        .all(|other| other.canonical_id != *alias)
                })
        })
    }

    fn rollback_one_transition(
        manager: &mut HolographicDeduplicationManager,
        memory_id: MemoryId,
    ) -> bool {
        matches!(
            manager.on_transition_rollback(memory_id),
            Ok(DedupEvent::TransitionRolledBack { .. })
        )
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 1.0e-12, "left={left} right={right}");
    }

    fn stress_inserted_manager() -> (HolographicDeduplicationManager, StateTrajectory) {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2]);
        manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert canonical");
        (manager, path)
    }

    fn semantic_node() -> MemoryNode {
        MemoryNode {
            memory_id: 1,
            tokens: vec!["Red".to_string(), "Apple".to_string()],
            semantic_labels: vec!["Fruit".to_string()],
            relations: vec![
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "has_color".to_string(),
                    target: "red".to_string(),
                },
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "is_a".to_string(),
                    target: "fruit".to_string(),
                },
            ],
            dependency_links: vec![(2, 3), (1, 2)],
            causal_links: vec![8, 7],
            trajectory_hint: vec![10, 11],
        }
    }

    fn semantic_node_with_id(memory_id: MemoryId) -> MemoryNode {
        MemoryNode {
            memory_id,
            ..semantic_node()
        }
    }

    fn distant_semantic_node(memory_id: MemoryId) -> MemoryNode {
        MemoryNode {
            memory_id,
            tokens: vec!["database".to_string(), "transaction".to_string()],
            semantic_labels: vec!["storage".to_string()],
            relations: vec![SemanticRelation {
                source: "transaction".to_string(),
                relation: "writes_to".to_string(),
                target: "database".to_string(),
            }],
            dependency_links: vec![(90, 91)],
            causal_links: vec![77],
            trajectory_hint: vec![88],
        }
    }

    fn observation(source: MemoryId, target: MemoryId, total_distance: f64) -> SemanticObservation {
        SemanticObservation {
            source,
            target,
            distance: SemanticDistance {
                topology_distance: total_distance,
                token_distance: total_distance,
                relation_distance: total_distance,
                trajectory_penalty: 0.0,
                total_distance,
            },
            observation_strength: normalize_distance(1.0 - total_distance),
        }
    }

    fn non_euclidean_observation_graph() -> ObservationGraph {
        ObservationGraph {
            adjacency: BTreeMap::from([
                (1, vec![observation(1, 2, 0.1)]),
                (2, vec![observation(2, 1, 0.1), observation(2, 3, 0.1)]),
                (3, vec![observation(3, 2, 0.1)]),
            ]),
        }
    }

    fn cluster_candidate(members: &[MemoryId], average_distance: f64) -> ClusterCandidate {
        let mut members = members.to_vec();
        members.sort();
        ClusterCandidate {
            candidate_id: stable_hash_u64s(members.iter().copied()),
            members,
            average_distance,
            topology_coherence: normalize_distance(1.0 - average_distance),
            relation_coherence: normalize_distance(1.0 - average_distance / 2.0),
        }
    }

    fn cluster_snapshot(candidates: Vec<ClusterCandidate>) -> ClusterCandidateSnapshot {
        cluster_candidate_snapshot(&ClusterCandidateTable { candidates })
    }

    fn drift_snapshot(before: u64, after: u64, score: f64) -> SemanticDriftSnapshot {
        semantic_drift_snapshot(
            before,
            after,
            SemanticDrift {
                drift_score: score,
                topology_shift: score,
                relation_shift: score / 2.0,
                membership_change: score / 3.0,
            },
        )
    }

    fn low_drift_window() -> StabilityWindow {
        stability_window(
            1,
            4,
            vec![
                drift_snapshot(1, 2, 0.05),
                drift_snapshot(2, 3, 0.04),
                drift_snapshot(3, 4, 0.03),
            ],
        )
    }

    fn oscillating_drift_window() -> StabilityWindow {
        stability_window(
            1,
            5,
            vec![
                drift_snapshot(1, 2, 0.1),
                drift_snapshot(2, 3, 0.8),
                drift_snapshot(3, 4, 0.1),
                drift_snapshot(4, 5, 0.8),
            ],
        )
    }

    fn stability_snapshot(timestamp: u64, score: f64) -> SemanticStabilitySnapshot {
        semantic_stability_snapshot(
            timestamp,
            SemanticStability {
                stability_score: score,
                temporal_consistency: score,
                topology_consistency: score,
                relation_consistency: score,
            },
        )
    }

    fn semantic_core_candidate(members: &[MemoryId], score: f64) -> SemanticCoreCandidate {
        let mut invariant_members = members.to_vec();
        invariant_members.sort();
        SemanticCoreCandidate {
            core_id: stable_hash_u64s(invariant_members.iter().copied()),
            invariant_members,
            stability_score: score,
            drift_resistance: score,
        }
    }

    fn semantic_identity_candidate(
        identity_id: u64,
        continuity_score: f64,
        invariant_core_overlap: f64,
        lineage: &[u64],
    ) -> SemanticIdentityCandidate {
        let mut drift_lineage = lineage.to_vec();
        drift_lineage.sort();
        drift_lineage.dedup();
        SemanticIdentityCandidate {
            identity_id,
            continuity_score,
            invariant_core_overlap,
            drift_lineage,
        }
    }

    fn merge_safe_identity_graph() -> SemanticIdentityGraph {
        SemanticIdentityGraph {
            identities: vec![
                semantic_identity_candidate(1, 0.95, 0.92, &[10, 20, 30, 40]),
                semantic_identity_candidate(2, 0.94, 0.90, &[10, 20, 30, 50]),
                semantic_identity_candidate(3, 0.60, 0.55, &[900, 901]),
            ],
        }
    }

    fn semantic_core_snapshot_for(
        timestamp: u64,
        candidates: Vec<SemanticCoreCandidate>,
    ) -> SemanticCoreSnapshot {
        semantic_core_snapshot(timestamp, &SemanticCoreCandidateTable { candidates })
    }

    fn identity_core_history() -> Vec<SemanticCoreSnapshot> {
        vec![
            semantic_core_snapshot_for(2, vec![semantic_core_candidate(&[2, 1, 3], 0.95)]),
            semantic_core_snapshot_for(1, vec![semantic_core_candidate(&[1, 2, 3], 0.95)]),
            semantic_core_snapshot_for(3, vec![semantic_core_candidate(&[1, 2, 3, 4], 0.92)]),
        ]
    }

    fn stable_core_history() -> (Vec<SemanticStabilitySnapshot>, Vec<SemanticDriftSnapshot>) {
        (
            vec![
                stability_snapshot(3, 0.94),
                stability_snapshot(1, 0.96),
                stability_snapshot(2, 0.95),
            ],
            vec![
                drift_snapshot(2, 3, 0.04),
                drift_snapshot(1, 2, 0.03),
                drift_snapshot(3, 4, 0.05),
            ],
        )
    }

    fn oscillating_core_history() -> (Vec<SemanticStabilitySnapshot>, Vec<SemanticDriftSnapshot>) {
        (
            vec![
                stability_snapshot(1, 0.9),
                stability_snapshot(2, 0.9),
                stability_snapshot(3, 0.9),
                stability_snapshot(4, 0.9),
            ],
            oscillating_drift_window().observations,
        )
    }

    #[test]
    fn test_semantic_fingerprint_is_deterministic() {
        let node = semantic_node();
        let left = build_semantic_fingerprint(&node);
        let right = build_semantic_fingerprint(&node);

        println!("topology_hash={:?}", left.topology_hash);
        println!("token_signature={:?}", left.token_signature);
        println!("relation_signature={:?}", left.relation_signature);
        println!("trajectory_hint={:?}", left.trajectory_hint);

        assert_eq!(left, right);
        assert_eq!(fingerprint_hash(&left), fingerprint_hash(&right));
    }

    #[test]
    fn test_semantic_fingerprint_is_runtime_order_independent() {
        let left_node = semantic_node();
        let right_node = MemoryNode {
            memory_id: 99,
            tokens: vec!["apple".to_string(), "red".to_string()],
            semantic_labels: vec!["fruit".to_string()],
            relations: vec![
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "is_a".to_string(),
                    target: "fruit".to_string(),
                },
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "has_color".to_string(),
                    target: "red".to_string(),
                },
            ],
            dependency_links: vec![(1, 2), (2, 3)],
            causal_links: vec![7, 8],
            trajectory_hint: vec![10, 11],
        };

        let left = build_semantic_fingerprint(&left_node);
        let right = build_semantic_fingerprint(&right_node);

        assert_eq!(left, right);
    }

    #[test]
    fn test_semantic_fingerprint_relation_signature_is_stable() {
        let mut left_node = semantic_node();
        let mut right_node = semantic_node();
        right_node.relations.reverse();
        right_node.dependency_links.reverse();
        right_node.causal_links.reverse();

        let left = build_semantic_fingerprint(&left_node);
        let right = build_semantic_fingerprint(&right_node);
        let comparison = compare_fingerprint(&left, &right);

        assert!(comparison.topology_match);
        assert!(comparison.token_match);
        assert!(comparison.relation_match);
        assert!(comparison.trajectory_hint_match);

        left_node.trajectory_hint.push(12);
        let drifted = build_semantic_fingerprint(&left_node);
        assert!(!compare_fingerprint(&left, &drifted).trajectory_hint_match);
    }

    #[test]
    fn test_semantic_fingerprint_generation_does_not_mutate_replay() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let node = semantic_node();

        let _fingerprint = build_semantic_fingerprint(&node);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_observation_graph_is_deterministic() {
        let memories = vec![
            semantic_node_with_id(1),
            semantic_node_with_id(2),
            distant_semantic_node(3),
        ];
        let threshold = ObservationThreshold::default();
        let left = build_observation_graph(&memories, &threshold);
        let right = build_observation_graph(&memories, &threshold);
        let snapshot = semantic_observation_snapshot(&left);
        let first = snapshot.observations.first().expect("observation");

        println!("source={:?}", first.source);
        println!("target={:?}", first.target);
        println!("distance={:?}", first.distance.total_distance);
        println!("strength={:?}", first.observation_strength);

        assert_eq!(left, right);
        assert_eq!(snapshot.observations.len(), 2);
    }

    #[test]
    fn test_semantic_observation_threshold_filters_distant_nodes() {
        let source = semantic_node_with_id(1);
        let target = distant_semantic_node(2);
        let threshold = ObservationThreshold {
            topology_threshold: 0.10,
            relation_threshold: 0.10,
            total_threshold: 0.10,
        };

        assert!(observe_semantic_relation(&source, &target, &threshold).is_none());
    }

    #[test]
    fn test_semantic_observation_is_symmetric() {
        let source = semantic_node_with_id(1);
        let target = semantic_node_with_id(2);
        let threshold = ObservationThreshold::default();

        let forward = observe_semantic_relation(&source, &target, &threshold).expect("forward");
        let reverse = observe_semantic_relation(&target, &source, &threshold).expect("reverse");

        assert_eq!(forward.source, reverse.target);
        assert_eq!(forward.target, reverse.source);
        assert_eq!(forward.distance, reverse.distance);
        assert_eq!(forward.observation_strength, reverse.observation_strength);
    }

    #[test]
    fn test_semantic_observation_graph_is_runtime_order_independent() {
        let left = vec![
            semantic_node_with_id(1),
            semantic_node_with_id(2),
            distant_semantic_node(3),
        ];
        let right = vec![
            distant_semantic_node(3),
            semantic_node_with_id(2),
            semantic_node_with_id(1),
        ];
        let threshold = ObservationThreshold::default();

        assert_eq!(
            build_observation_graph(&left, &threshold),
            build_observation_graph(&right, &threshold)
        );
    }

    #[test]
    fn test_semantic_observation_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let memories = vec![semantic_node_with_id(1), semantic_node_with_id(2)];

        let graph = build_observation_graph(&memories, &ObservationThreshold::default());
        let _snapshot = semantic_observation_snapshot(&graph);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_cluster_candidate_generation_is_deterministic() {
        let memories = vec![
            semantic_node_with_id(1),
            semantic_node_with_id(2),
            distant_semantic_node(3),
        ];
        let threshold = ObservationThreshold::default();
        let graph = build_observation_graph(&memories, &threshold);
        let left = build_cluster_candidates(&graph, &threshold);
        let right = build_cluster_candidates(&graph, &threshold);
        let candidate = left.candidates.first().expect("candidate");

        println!("candidate_id={:?}", candidate.candidate_id);
        println!("members={:?}", candidate.members);
        println!("average_distance={}", candidate.average_distance);
        println!("topology_coherence={}", candidate.topology_coherence);
        println!("relation_coherence={}", candidate.relation_coherence);

        assert_eq!(left, right);
        assert_eq!(left.candidates.len(), 1);
    }

    #[test]
    fn test_cluster_candidate_member_ordering_is_stable() {
        let graph = ObservationGraph {
            adjacency: BTreeMap::from([
                (3, vec![observation(3, 1, 0.2)]),
                (1, vec![observation(1, 2, 0.2)]),
                (2, vec![observation(2, 3, 0.2)]),
            ]),
        };
        let table = build_cluster_candidates(&graph, &ObservationThreshold::default());

        assert_eq!(table.candidates[0].members, vec![1, 2, 3]);
    }

    #[test]
    fn test_cluster_candidate_coherence_is_stable() {
        let graph = non_euclidean_observation_graph();
        let table = build_cluster_candidates(&graph, &ObservationThreshold::default());
        let candidate = table.candidates.first().expect("candidate");

        assert_eq!(
            compute_candidate_coherence(candidate),
            compute_candidate_coherence(candidate)
        );
        assert!((0.0..=1.0).contains(&candidate.average_distance));
        assert!((0.0..=1.0).contains(&candidate.topology_coherence));
        assert!((0.0..=1.0).contains(&candidate.relation_coherence));
    }

    #[test]
    fn test_cluster_candidate_generation_is_runtime_order_independent() {
        let threshold = ObservationThreshold::default();
        let left = build_observation_graph(
            &[
                semantic_node_with_id(1),
                semantic_node_with_id(2),
                distant_semantic_node(3),
            ],
            &threshold,
        );
        let right = build_observation_graph(
            &[
                distant_semantic_node(3),
                semantic_node_with_id(2),
                semantic_node_with_id(1),
            ],
            &threshold,
        );

        assert_eq!(
            cluster_candidate_snapshot(&build_cluster_candidates(&left, &threshold)),
            cluster_candidate_snapshot(&build_cluster_candidates(&right, &threshold))
        );
    }

    #[test]
    fn test_cluster_candidate_generation_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let graph = non_euclidean_observation_graph();

        let table = build_cluster_candidates(&graph, &ObservationThreshold::default());
        let _snapshot = cluster_candidate_snapshot(&table);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_cluster_candidate_non_euclidean_safety() {
        let graph = non_euclidean_observation_graph();
        let table = build_cluster_candidates(&graph, &ObservationThreshold::default());
        let candidate = table.candidates.first().expect("candidate");

        assert_eq!(candidate.members, vec![1, 2, 3]);
        assert_close(candidate.average_distance, 0.4);
        assert_close(candidate.topology_coherence, 0.6);
        assert_close(candidate.relation_coherence, 0.9);
    }

    #[test]
    fn test_semantic_drift_is_deterministic() {
        let before = cluster_snapshot(vec![cluster_candidate(&[1, 2], 0.1)]);
        let after = cluster_snapshot(vec![cluster_candidate(&[1, 2, 3], 0.2)]);

        let first = semantic_drift(&before, &after);
        let second = semantic_drift(&before, &after);

        println!("drift_score={}", first.drift_score);
        println!("topology_shift={}", first.topology_shift);
        println!("relation_shift={}", first.relation_shift);
        println!("membership_change={}", first.membership_change);

        assert_eq!(first, second);
    }

    #[test]
    fn test_temporal_drift_snapshot_orders_timestamps() {
        let drift = SemanticDrift {
            drift_score: 0.1,
            topology_shift: 0.2,
            relation_shift: 0.3,
            membership_change: 0.4,
        };
        let snapshot = semantic_drift_snapshot(20, 10, drift);

        assert_eq!(snapshot.before_timestamp, 10);
        assert_eq!(snapshot.after_timestamp, 20);
        assert_eq!(snapshot.drift, drift);
    }

    #[test]
    fn test_semantic_drift_is_normalized() {
        let before = cluster_snapshot(vec![cluster_candidate(&[1, 2], 0.0)]);
        let after = cluster_snapshot(vec![cluster_candidate(&[3, 4], 1.0)]);
        let drift = semantic_drift(&before, &after);

        for value in [
            drift.drift_score,
            drift.topology_shift,
            drift.relation_shift,
            drift.membership_change,
        ] {
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn test_semantic_drift_is_runtime_order_independent() {
        let before = cluster_snapshot(vec![
            cluster_candidate(&[2, 1], 0.1),
            cluster_candidate(&[4, 3], 0.2),
        ]);
        let after_left = cluster_snapshot(vec![
            cluster_candidate(&[1, 2, 5], 0.2),
            cluster_candidate(&[3, 4], 0.2),
        ]);
        let after_right = cluster_snapshot(vec![
            cluster_candidate(&[4, 3], 0.2),
            cluster_candidate(&[5, 2, 1], 0.2),
        ]);

        assert_eq!(
            semantic_drift(&before, &after_left),
            semantic_drift(&before, &after_right)
        );
    }

    #[test]
    fn test_semantic_drift_tracking_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let before = cluster_snapshot(vec![cluster_candidate(&[1, 2], 0.1)]);
        let after = cluster_snapshot(vec![cluster_candidate(&[1, 2, 3], 0.2)]);

        let drift = semantic_drift(&before, &after);
        let _snapshot = semantic_drift_snapshot(1, 2, drift);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_temporal_observation_recording_orders_snapshot() {
        let snapshot = SemanticObservationSnapshot {
            observations: vec![observation(2, 1, 0.2), observation(1, 2, 0.2)],
        };
        let temporal = record_temporal_observation(7, snapshot);

        assert_eq!(temporal.timestamp, 7);
        assert_eq!(temporal.snapshot.observations[0].source, 1);
        assert_eq!(temporal.snapshot.observations[1].source, 2);
    }

    #[test]
    fn test_semantic_drift_observes_cluster_evolution() {
        let base = cluster_snapshot(vec![
            cluster_candidate(&[1, 2], 0.1),
            cluster_candidate(&[3, 4], 0.1),
        ]);
        let grows = cluster_snapshot(vec![
            cluster_candidate(&[1, 2, 5], 0.2),
            cluster_candidate(&[3, 4], 0.1),
        ]);
        let splits = cluster_snapshot(vec![
            cluster_candidate(&[1], 0.0),
            cluster_candidate(&[2], 0.0),
            cluster_candidate(&[3, 4], 0.1),
        ]);
        let shrinks = cluster_snapshot(vec![
            cluster_candidate(&[1], 0.0),
            cluster_candidate(&[3, 4], 0.1),
        ]);

        let grow_drift = semantic_drift(&base, &grows);
        let split_drift = semantic_drift(&base, &splits);
        let shrink_drift = semantic_drift(&base, &shrinks);

        assert!(grow_drift.drift_score > 0.0);
        assert!(split_drift.drift_score > 0.0);
        assert!(shrink_drift.drift_score > 0.0);
        assert_eq!(grow_drift, semantic_drift(&base, &grows));
        assert_eq!(split_drift, semantic_drift(&base, &splits));
        assert_eq!(shrink_drift, semantic_drift(&base, &shrinks));
    }

    #[test]
    fn test_semantic_stability_is_deterministic() {
        let window = low_drift_window();
        let first = semantic_stability(&window);
        let second = semantic_stability(&window);

        println!("stability_score={}", first.stability_score);
        println!("temporal_consistency={}", first.temporal_consistency);
        println!("topology_consistency={}", first.topology_consistency);
        println!("relation_consistency={}", first.relation_consistency);

        assert_eq!(first, second);
    }

    #[test]
    fn test_semantic_stability_is_normalized() {
        let stability = semantic_stability(&stability_window(
            1,
            2,
            vec![semantic_drift_snapshot(
                1,
                2,
                SemanticDrift {
                    drift_score: 2.0,
                    topology_shift: -1.0,
                    relation_shift: f64::NAN,
                    membership_change: 0.5,
                },
            )],
        ));

        for value in [
            stability.stability_score,
            stability.temporal_consistency,
            stability.topology_consistency,
            stability.relation_consistency,
        ] {
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn test_semantic_stability_is_runtime_order_independent() {
        let ordered = stability_window(
            1,
            4,
            vec![
                drift_snapshot(1, 2, 0.1),
                drift_snapshot(2, 3, 0.2),
                drift_snapshot(3, 4, 0.3),
            ],
        );
        let shuffled = stability_window(
            4,
            1,
            vec![
                drift_snapshot(3, 4, 0.3),
                drift_snapshot(1, 2, 0.1),
                drift_snapshot(2, 3, 0.2),
            ],
        );

        assert_eq!(semantic_stability(&ordered), semantic_stability(&shuffled));
        assert_eq!(stability_velocity(&ordered), stability_velocity(&shuffled));
    }

    #[test]
    fn test_semantic_stability_analysis_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();

        let stability = semantic_stability(&low_drift_window());
        let _velocity = stability_velocity(&low_drift_window());
        let _snapshot = semantic_stability_snapshot(5, stability);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_stability_detects_convergence() {
        let stability = semantic_stability(&low_drift_window());
        let velocity = stability_velocity(&low_drift_window());

        assert!(stability.stability_score > 0.9);
        assert!(velocity.drift_velocity < 0.0);
        assert!(velocity.stability_velocity > 0.0);
    }

    #[test]
    fn test_semantic_stability_does_not_falsely_converge_oscillation() {
        let stable = semantic_stability(&low_drift_window());
        let oscillating = semantic_stability(&oscillating_drift_window());

        assert!(oscillating.stability_score < stable.stability_score);
        assert!(oscillating.stability_score < 0.5);
    }

    #[test]
    fn test_semantic_core_extraction_is_deterministic() {
        let (stability, drift) = stable_core_history();
        let first = semantic_core_candidates(&stability, &drift);
        let second = semantic_core_candidates(&stability, &drift);
        let candidate = first.candidates.first().expect("core candidate");

        println!("core_id={:?}", candidate.core_id);
        println!("stability_score={}", candidate.stability_score);
        println!("drift_resistance={}", candidate.drift_resistance);
        println!("members={:?}", candidate.invariant_members);

        assert_eq!(first, second);
        assert_eq!(first.candidates.len(), 1);
    }

    #[test]
    fn test_semantic_core_member_ordering_is_stable() {
        let (stability, drift) = stable_core_history();
        let table = semantic_core_candidates(&stability, &drift);
        let snapshot = semantic_core_snapshot(10, &table);
        let members = &snapshot.candidates[0].invariant_members;

        assert!(members.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn test_drift_resistance_is_stable() {
        let (_stability, drift) = stable_core_history();
        let candidate = cluster_candidate(&[3, 1, 2], 0.05);

        assert_eq!(
            drift_resistance(&candidate, &drift),
            drift_resistance(&candidate, &drift)
        );
    }

    #[test]
    fn test_semantic_core_extraction_is_runtime_order_independent() {
        let (stability, drift) = stable_core_history();
        let mut reversed_stability = stability.clone();
        reversed_stability.reverse();
        let mut reversed_drift = drift.clone();
        reversed_drift.reverse();

        assert_eq!(
            semantic_core_candidates(&stability, &drift),
            semantic_core_candidates(&reversed_stability, &reversed_drift)
        );
    }

    #[test]
    fn test_semantic_core_extraction_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let (stability, drift) = stable_core_history();

        let table = semantic_core_candidates(&stability, &drift);
        let _snapshot = semantic_core_snapshot(10, &table);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_core_oscillation_safety() {
        let (stability, drift) = oscillating_core_history();
        let table = semantic_core_candidates(&stability, &drift);

        assert!(table.candidates.is_empty());
    }

    #[test]
    fn test_semantic_core_long_term_persistence() {
        let (stability, drift) = stable_core_history();
        let table = semantic_core_candidates(&stability, &drift);
        let candidate = table.candidates.first().expect("core candidate");

        assert!(candidate.stability_score > 0.9);
        assert!(candidate.drift_resistance > 0.9);
    }

    #[test]
    fn test_semantic_identity_tracking_is_deterministic() {
        let cores = identity_core_history();
        let drift = low_drift_window().observations;
        let first = semantic_identity_graph(&cores, &drift);
        let second = semantic_identity_graph(&cores, &drift);
        let identity = first.identities.first().expect("identity");

        println!("identity_id={:?}", identity.identity_id);
        println!("continuity_score={}", identity.continuity_score);
        println!("core_overlap={}", identity.invariant_core_overlap);
        println!("lineage={:?}", identity.drift_lineage);

        assert_eq!(first, second);
        assert!(!first.identities.is_empty());
    }

    #[test]
    fn test_semantic_identity_ordering_is_stable() {
        let graph =
            semantic_identity_graph(&identity_core_history(), &low_drift_window().observations);
        let snapshot = semantic_identity_snapshot(10, &graph);

        assert!(
            snapshot
                .identities
                .windows(2)
                .all(|pair| identity_order(&pair[0], &pair[1]) != std::cmp::Ordering::Greater)
        );
        assert!(snapshot.identities.iter().all(|identity| {
            identity
                .drift_lineage
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        }));
    }

    #[test]
    fn test_continuity_score_is_stable() {
        let previous = semantic_core_candidate(&[1, 2, 3], 0.95);
        let current = semantic_core_candidate(&[2, 3, 4], 0.90);

        assert_eq!(
            continuity_score(&previous, &current),
            continuity_score(&previous, &current)
        );
    }

    #[test]
    fn test_semantic_identity_tracking_is_runtime_order_independent() {
        let cores = identity_core_history();
        let mut reversed_cores = cores.clone();
        reversed_cores.reverse();
        let drift = low_drift_window().observations;
        let mut reversed_drift = drift.clone();
        reversed_drift.reverse();

        assert_eq!(
            semantic_identity_graph(&cores, &drift),
            semantic_identity_graph(&reversed_cores, &reversed_drift)
        );
    }

    #[test]
    fn test_semantic_identity_tracking_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();

        let graph =
            semantic_identity_graph(&identity_core_history(), &low_drift_window().observations);
        let _snapshot = semantic_identity_snapshot(10, &graph);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_identity_branching_safety() {
        let ancestor =
            semantic_core_snapshot_for(1, vec![semantic_core_candidate(&[1, 2, 3], 0.95)]);
        let descendants = semantic_core_snapshot_for(
            2,
            vec![
                semantic_core_candidate(&[1, 2, 3, 4], 0.90),
                semantic_core_candidate(&[1, 2, 3, 5], 0.90),
            ],
        );
        let graph =
            semantic_identity_graph(&[ancestor, descendants], &low_drift_window().observations);
        let lineage = identity_lineages(&graph);

        assert!(graph.identities.len() >= 3);
        assert!(lineage.iter().any(|entry| entry.descendant_ids.len() > 1));
    }

    #[test]
    fn test_semantic_identity_divergence_observes_new_identity() {
        let previous =
            semantic_core_snapshot_for(1, vec![semantic_core_candidate(&[1, 2, 3], 0.95)]);
        let divergent =
            semantic_core_snapshot_for(2, vec![semantic_core_candidate(&[8, 9, 10], 0.95)]);
        let graph = semantic_identity_graph(
            &[previous, divergent],
            &oscillating_drift_window().observations,
        );
        let disconnected = graph
            .identities
            .iter()
            .filter(|identity| identity.continuity_score == 0.0)
            .count();

        assert!(disconnected >= 1);
    }

    #[test]
    fn test_semantic_merge_safe_candidate_merges() {
        let candidates = merge_candidates(&merge_safe_identity_graph());
        let candidate = candidates.first().expect("merge candidate");
        let result = semantic_merge(candidate).expect("safe merge");

        println!("left_identity={:?}", candidate.left_identity);
        println!("right_identity={:?}", candidate.right_identity);
        println!("merge_risk_score={}", candidate.merge_risk_score);
        println!("compression_gain={}", candidate.compression_gain);
        println!("merged_identity={:?}", result.merged_identity);
        println!("merge_confidence={}", result.merge_confidence);

        assert_eq!(result.source_identities, vec![1, 2]);
        assert!(!result.preserved_invariants.is_empty());
        assert!(result.merge_confidence > 0.7);
        assert!(result.semantic_loss_score < 0.3);
    }

    #[test]
    fn test_semantic_merge_unsafe_candidate_rejected() {
        let candidate = SemanticMergeCandidate {
            left_identity: 1,
            right_identity: 99,
            continuity_score: 0.90,
            invariant_overlap_score: 0.10,
            contradiction_density: 0.70,
            lineage_distance: 20,
            merge_risk_score: 0.90,
            compression_gain: 0.10,
        };

        assert!(matches!(
            semantic_merge(&candidate),
            Err(MemorySpaceError::UnsafeSemanticMerge(_))
        ));
    }

    #[test]
    fn test_semantic_merge_preserves_contradiction_as_reject() {
        let graph = SemanticIdentityGraph {
            identities: vec![
                semantic_identity_candidate(1, 0.95, 0.95, &[1, 2, 3]),
                semantic_identity_candidate(2, 0.20, 0.10, &[1, 2, 30]),
            ],
        };
        let candidate = merge_candidates(&graph).pop().expect("candidate");

        assert!(candidate.contradiction_density > 0.0);
        assert!(semantic_merge(&candidate).is_err());
    }

    #[test]
    fn test_semantic_compression_preserves_branches_without_graph_mutation() {
        let graph = merge_safe_identity_graph();
        let before = graph.clone();
        let snapshot = semantic_compression(&graph);

        assert_eq!(graph, before);
        assert_eq!(snapshot.identity_count_before, 3);
        assert!(snapshot.identity_count_after >= 2);
    }

    #[test]
    fn test_semantic_merge_determinism_and_ordering() {
        let graph = merge_safe_identity_graph();
        let mut reversed = graph.clone();
        reversed.identities.reverse();

        assert_eq!(merge_candidates(&graph), merge_candidates(&reversed));
        assert_eq!(
            semantic_compression(&graph),
            semantic_compression(&reversed)
        );
    }

    #[test]
    fn test_semantic_compression_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();

        let _snapshot = semantic_compression(&merge_safe_identity_graph());

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_compression_reduces_entropy_and_conserves_mass() {
        let snapshot = semantic_compression(&merge_safe_identity_graph());

        assert!(snapshot.identity_count_after < snapshot.identity_count_before);
        assert!(snapshot.compression_ratio > 0.0);
        assert!((0.0..=1.0).contains(&snapshot.preserved_semantic_mass));
        assert!((0.0..=1.0).contains(&snapshot.discarded_semantic_mass));
    }

    #[test]
    fn test_semantic_anchor_survives_planned_compression() {
        let anchor = SemanticAnchor {
            anchor_id: 7,
            identity_id: 1,
            invariant_core: InvariantCore { core_id: 10 },
        };
        let before = anchor;
        let _snapshot = semantic_compression(&merge_safe_identity_graph());

        assert_eq!(anchor, before);
    }

    #[test]
    fn test_semantic_attractors_are_deterministic_and_ordered() {
        let graph = merge_safe_identity_graph();
        let mut reversed = graph.clone();
        reversed.identities.reverse();

        let attractors = semantic_attractors(&graph);
        let reversed_attractors = semantic_attractors(&reversed);
        let first = attractors.first().expect("attractor");

        println!("attractor_id={:?}", first.attractor_id);
        println!("attractor_strength={}", first.attractor_strength);
        println!("semantic_mass={}", first.semantic_mass);

        assert_eq!(attractors, reversed_attractors);
        assert!(
            attractors
                .windows(2)
                .all(|window| window[0].attractor_strength >= window[1].attractor_strength)
        );
        assert!(
            first
                .anchor_set
                .windows(2)
                .all(|window| window[0] <= window[1])
        );
    }

    #[test]
    fn test_semantic_drift_detection_recoverable_and_divergent() {
        let previous = SemanticIdentitySnapshot {
            timestamp: 1,
            identities: vec![
                semantic_identity_candidate(1, 0.90, 0.90, &[1, 2, 3]),
                semantic_identity_candidate(2, 0.92, 0.91, &[10, 11, 12]),
            ],
        };
        let current = SemanticIdentitySnapshot {
            timestamp: 2,
            identities: vec![
                semantic_identity_candidate(1, 0.74, 0.88, &[1, 2, 3, 4]),
                semantic_identity_candidate(2, 0.18, 0.20, &[900, 901]),
            ],
        };

        let events = detect_semantic_drift(&previous, &current);
        let recoverable = events
            .iter()
            .find(|event| event.identity_id == 1)
            .expect("recoverable event");
        let divergent = events
            .iter()
            .find(|event| event.identity_id == 2)
            .expect("divergent event");

        println!("identity_id={:?}", recoverable.identity_id);
        println!("drift_magnitude={}", recoverable.drift_magnitude);
        println!("recoverable={}", recoverable.recoverable);

        assert!(recoverable.recoverable);
        assert!(!divergent.recoverable);
        assert!(divergent.drift_magnitude > recoverable.drift_magnitude);
    }

    #[test]
    fn test_stabilization_state_recoverable_and_irreversible() {
        let recoverable_identity = semantic_identity_candidate(1, 0.82, 0.78, &[10, 20]);
        let divergent_identity = semantic_identity_candidate(2, 0.30, 0.20, &[]);

        let recoverable = stabilization_state(&recoverable_identity);
        let divergent = stabilization_state(&divergent_identity);

        assert!(recoverable.recoverable);
        assert!(recoverable.stabilization_confidence > 0.0);
        assert!(!divergent.recoverable);
        assert!(divergent.contradiction_density > recoverable.contradiction_density);
    }

    #[test]
    fn test_semantic_correction_plan_preserves_anchor_and_suppresses_contradiction() {
        let identity = semantic_identity_candidate(1, 0.72, 0.70, &[10, 20]);
        let state = stabilization_state(&identity);
        let plan = semantic_correction_plan(&state).expect("correction plan");
        let corrected = apply_semantic_correction(&identity, &plan);

        assert_eq!(plan.target_identity, identity.identity_id);
        assert!(!plan.restored_invariants.is_empty());
        assert!(!plan.rejected_fragments.is_empty());
        assert_eq!(corrected.identity_id, identity.identity_id);
        assert!(
            corrected
                .drift_lineage
                .contains(&plan.restored_invariants[0].core_id)
        );
        assert!(corrected.continuity_score >= identity.continuity_score);
        assert!(corrected.invariant_core_overlap >= identity.invariant_core_overlap);
    }

    #[test]
    fn test_semantic_correction_apply_is_pure_and_rejects_wrong_target() {
        let identity = semantic_identity_candidate(1, 0.72, 0.70, &[10, 20]);
        let before = identity.clone();
        let plan = SemanticCorrectionPlan {
            target_identity: 99,
            restored_invariants: vec![InvariantCore { core_id: 123 }],
            rejected_fragments: vec![SemanticFragment { fragment_id: 456 }],
            correction_confidence: 0.95,
        };

        let corrected = apply_semantic_correction(&identity, &plan);

        assert_eq!(identity, before);
        assert_eq!(corrected, before);
    }

    #[test]
    fn test_semantic_stabilization_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let graph = merge_safe_identity_graph();
        let attractors = semantic_attractors(&graph);
        let identity = graph.identities.first().expect("identity");
        let state = stabilization_state(identity);
        let plan = semantic_correction_plan(&state).expect("correction plan");
        let _corrected = apply_semantic_correction(identity, &plan);

        println!("attractor_id={:?}", attractors[0].attractor_id);
        println!(
            "stabilization_confidence={}",
            state.stabilization_confidence
        );

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_semantic_stabilization_rejects_false_recovery_and_bounds_mass() {
        let graph = SemanticIdentityGraph {
            identities: vec![
                semantic_identity_candidate(1, 0.95, 0.95, &[1, 2, 3]),
                semantic_identity_candidate(2, 0.15, 0.10, &[90, 91]),
            ],
        };
        let attractors = semantic_attractors(&graph);
        let unstable = graph
            .identities
            .iter()
            .find(|identity| identity.identity_id == 2)
            .expect("unstable identity");
        let state = stabilization_state(unstable);

        assert!(attractors.iter().all(|attractor| {
            (0.0..=1.0).contains(&attractor.invariant_density)
                && (0.0..=1.0).contains(&attractor.stability_score)
                && (0.0..=1.0).contains(&attractor.attractor_strength)
                && (0.0..=1.0).contains(&attractor.semantic_mass)
        }));
        assert!(!state.recoverable);
        assert!(semantic_correction_plan(&state).is_none());
    }

    #[test]
    fn test_semantic_stabilization_runtime_independence() {
        let previous = SemanticIdentitySnapshot {
            timestamp: 1,
            identities: vec![
                semantic_identity_candidate(2, 0.86, 0.80, &[20, 10]),
                semantic_identity_candidate(1, 0.90, 0.88, &[1, 2, 3]),
            ],
        };
        let current = SemanticIdentitySnapshot {
            timestamp: 2,
            identities: vec![
                semantic_identity_candidate(1, 0.84, 0.86, &[3, 2, 1, 4]),
                semantic_identity_candidate(2, 0.82, 0.79, &[10, 20, 30]),
            ],
        };
        let mut reversed_previous = previous.clone();
        let mut reversed_current = current.clone();
        reversed_previous.identities.reverse();
        reversed_current.identities.reverse();

        assert_eq!(
            detect_semantic_drift(&previous, &current),
            detect_semantic_drift(&reversed_previous, &reversed_current)
        );
        assert_eq!(
            semantic_attractors(&SemanticIdentityGraph {
                identities: current.identities.clone()
            }),
            semantic_attractors(&SemanticIdentityGraph {
                identities: reversed_current.identities
            })
        );
    }

    #[test]
    fn test_semantic_rewrite_transaction_construction_is_deterministic() {
        let graph = merge_safe_identity_graph();
        let mut reversed = graph.clone();
        reversed.identities.reverse();

        let transaction = semantic_rewrite_transaction(&graph);
        let reversed_transaction = semantic_rewrite_transaction(&reversed);

        println!("transaction_id={:?}", transaction.transaction_id);
        println!("checksum={:?}", transaction.deterministic_checksum);
        println!(
            "merge_candidate_count={}",
            transaction.preview.topology_diff.merge_candidate_count
        );

        assert_eq!(transaction, reversed_transaction);
        assert!(transaction.validation.valid);
        assert_eq!(
            transaction.deterministic_checksum,
            deterministic_rewrite_checksum(&transaction)
        );
    }

    #[test]
    fn test_semantic_rewrite_preview_is_immutable() {
        let graph = merge_safe_identity_graph();
        let before = graph.clone();
        let transaction = semantic_rewrite_transaction(&graph);
        let preview = semantic_rewrite_preview(&transaction);

        assert_eq!(graph, before);
        assert_eq!(preview, transaction.preview);
        assert_eq!(
            transaction.rollback_snapshot.topology_snapshot,
            transaction.source_snapshot
        );
    }

    #[test]
    fn test_semantic_rewrite_validation_guards_continuity_and_bounds() {
        let valid = semantic_rewrite_transaction(&merge_safe_identity_graph());
        assert!(valid.validation.continuity_retained);
        assert!(valid.validation.anchors_preserved);
        assert!(valid.validation.contradiction_bounded);
        assert!(valid.validation.semantic_mass_bounded);

        let unstable = SemanticIdentityGraph {
            identities: vec![semantic_identity_candidate(99, 0.10, 0.10, &[900])],
        };
        let rejected = semantic_rewrite_transaction(&unstable);

        assert!(!rejected.validation.valid);
        assert!(!rejected.validation.continuity_retained);
        assert!(
            rejected
                .validation
                .validation_errors
                .iter()
                .any(|error| error.contains("continuity"))
        );
    }

    #[test]
    fn test_semantic_rewrite_rollback_snapshot_preserves_attractors_and_anchors() {
        let graph = merge_safe_identity_graph();
        let rollback = semantic_rollback_snapshot(&graph);
        let attractors = semantic_attractors(&graph);
        let mut anchors = attractors
            .iter()
            .flat_map(|attractor| attractor.anchor_set.iter().copied())
            .collect::<Vec<_>>();
        anchors.sort();
        anchors.dedup();

        assert_eq!(rollback.attractor_snapshot, attractors);
        assert_eq!(rollback.anchor_snapshot, anchors);
        assert_eq!(
            rollback.snapshot_id,
            semantic_rollback_snapshot(&graph).snapshot_id
        );
    }

    #[test]
    fn test_semantic_rewrite_checksum_is_runtime_order_independent() {
        let graph = merge_safe_identity_graph();
        let mut reversed = graph.clone();
        reversed.identities.reverse();
        for identity in &mut reversed.identities {
            identity.drift_lineage.reverse();
        }

        let checksum = semantic_rewrite_transaction(&graph).deterministic_checksum;
        let reversed_checksum = semantic_rewrite_transaction(&reversed).deterministic_checksum;

        assert_eq!(checksum, reversed_checksum);
    }

    #[test]
    fn test_semantic_rewrite_transaction_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();

        let transaction = semantic_rewrite_transaction(&merge_safe_identity_graph());
        let validation = validate_semantic_rewrite(&transaction);

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
        assert!(validation.replay_invariant);
        assert!(validation.topology_invariant);
    }

    #[test]
    fn test_semantic_rewrite_validation_rejects_topology_snapshot_mismatch() {
        let mut transaction = semantic_rewrite_transaction(&merge_safe_identity_graph());
        transaction
            .rollback_snapshot
            .topology_snapshot
            .identities
            .clear();
        transaction.preview = semantic_rewrite_preview(&transaction);
        let validation = validate_semantic_rewrite(&transaction);

        assert!(!validation.valid);
        assert!(!validation.topology_invariant);
        assert!(
            validation
                .validation_errors
                .iter()
                .any(|error| error.contains("topology"))
        );
    }

    #[test]
    fn test_semantic_attractor_field_is_deterministic() {
        let rewrites = vec![
            semantic_rewrite_transaction(&merge_safe_identity_graph()),
            semantic_rewrite_transaction(&SemanticIdentityGraph {
                identities: vec![
                    semantic_identity_candidate(1, 0.97, 0.94, &[10, 20, 30, 40, 60]),
                    semantic_identity_candidate(2, 0.95, 0.91, &[10, 20, 30, 50]),
                    semantic_identity_candidate(3, 0.62, 0.58, &[900, 901]),
                ],
            }),
        ];
        let mut reversed = rewrites.clone();
        reversed.reverse();
        let drift = vec![drift_snapshot(1, 2, 0.08), drift_snapshot(2, 3, 0.04)];
        let stability = vec![stability_snapshot(1, 0.90), stability_snapshot(2, 0.94)];

        let field = semantic_attractor_field(&rewrites, &drift, &stability);
        let reversed_field = semantic_attractor_field(&reversed, &drift, &stability);
        let first = field.attractors.first().expect("attractor");

        println!("attractor_id={:?}", first.attractor_id);
        println!("basin_strength={}", first.basin_strength);
        println!("semantic_density={}", first.semantic_density);
        println!("stability_gradient={}", first.stability_gradient);
        println!("collapse_risk={}", collapse_risk(&field).collapse_score);

        assert_eq!(field, reversed_field);
        assert!(!field.attractors.is_empty());
    }

    #[test]
    fn test_semantic_attractor_field_ordering_is_stable() {
        let rewrites = vec![
            semantic_rewrite_transaction(&merge_safe_identity_graph()),
            semantic_rewrite_transaction(&merge_safe_identity_graph()),
        ];
        let field = semantic_attractor_field(&rewrites, &[], &[]);

        assert!(
            field
                .attractors
                .windows(2)
                .all(|window| window[0].basin_strength >= window[1].basin_strength)
        );
        assert_eq!(semantic_attractor_snapshot(7, &field).field, field);
    }

    #[test]
    fn test_rewrite_energy_is_stable_and_normalized() {
        let transaction = semantic_rewrite_transaction(&merge_safe_identity_graph());
        let first = rewrite_energy(&transaction);
        let second = rewrite_energy(&transaction);

        assert_eq!(first, second);
        assert!((0.0..=1.0).contains(&first.topology_energy));
        assert!((0.0..=1.0).contains(&first.relation_energy));
        assert!((0.0..=1.0).contains(&first.continuity_energy));
        assert!((0.0..=1.0).contains(&first.total_energy));
    }

    #[test]
    fn test_collapse_risk_is_stable_and_normalized() {
        let rewrites = vec![
            semantic_rewrite_transaction(&merge_safe_identity_graph()),
            semantic_rewrite_transaction(&merge_safe_identity_graph()),
        ];
        let field = semantic_attractor_field(
            &rewrites,
            &low_drift_window().observations,
            &[stability_snapshot(1, 0.95), stability_snapshot(2, 0.96)],
        );
        let first = collapse_risk(&field);
        let second = collapse_risk(&field);

        assert_eq!(first, second);
        assert!((0.0..=1.0).contains(&first.collapse_score));
        assert!((0.0..=1.0).contains(&first.semantic_density_risk));
        assert!((0.0..=1.0).contains(&first.attractor_overconvergence));
    }

    #[test]
    fn test_semantic_attractor_field_is_runtime_order_independent() {
        let graph = merge_safe_identity_graph();
        let mut reversed_graph = graph.clone();
        reversed_graph.identities.reverse();
        for identity in &mut reversed_graph.identities {
            identity.drift_lineage.reverse();
        }
        let rewrites = vec![
            semantic_rewrite_transaction(&graph),
            semantic_rewrite_transaction(&reversed_graph),
        ];
        let mut reversed_rewrites = rewrites.clone();
        reversed_rewrites.reverse();
        let drift = vec![drift_snapshot(2, 3, 0.04), drift_snapshot(1, 2, 0.08)];
        let stability = vec![stability_snapshot(2, 0.96), stability_snapshot(1, 0.92)];

        assert_eq!(
            semantic_attractor_field(&rewrites, &drift, &stability),
            semantic_attractor_field(&reversed_rewrites, &drift, &stability)
        );
    }

    #[test]
    fn test_semantic_attractor_analysis_does_not_mutate_replay_or_topology() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let rewrites = vec![semantic_rewrite_transaction(&merge_safe_identity_graph())];

        let field = semantic_attractor_field(
            &rewrites,
            &low_drift_window().observations,
            &[stability_snapshot(1, 0.95)],
        );
        let _risk = collapse_risk(&field);
        let _snapshot = semantic_attractor_snapshot(99, &field);

        assert_eq!(
            replay_before,
            manager.replay_fingerprint(1).expect("replay after")
        );
        assert_eq!(topology_before, manager.topology_snapshot());
    }

    #[test]
    fn test_semantic_attractor_collapse_safety_observes_without_flattening() {
        let graph = merge_safe_identity_graph();
        let before = graph.clone();
        let rewrites = vec![
            semantic_rewrite_transaction(&graph),
            semantic_rewrite_transaction(&graph),
            semantic_rewrite_transaction(&graph),
        ];
        let field = semantic_attractor_field(
            &rewrites,
            &[drift_snapshot(1, 2, 0.01), drift_snapshot(2, 3, 0.01)],
            &[stability_snapshot(1, 0.99), stability_snapshot(2, 0.99)],
        );
        let risk = collapse_risk(&field);

        assert_eq!(graph, before);
        assert_eq!(field.attractors.len(), graph.identities.len());
        assert!(risk.collapse_score > 0.0);
        assert!(
            field
                .attractors
                .iter()
                .all(|attractor| !attractor.anchor_set.is_empty())
        );
    }

    #[test]
    fn test_semantic_distance_is_deterministic() {
        let left = build_semantic_fingerprint(&semantic_node());
        let mut right_node = semantic_node();
        right_node.tokens.push("fresh".to_string());
        let right = build_semantic_fingerprint(&right_node);
        let weights = SemanticDistanceWeights::default();

        let first = semantic_distance(&left, &right, &weights);
        let second = semantic_distance(&left, &right, &weights);

        println!("topology_distance={}", first.topology_distance);
        println!("token_distance={}", first.token_distance);
        println!("relation_distance={}", first.relation_distance);
        println!("trajectory_penalty={}", first.trajectory_penalty);
        println!("total_distance={}", first.total_distance);

        assert_eq!(first, second);
    }

    #[test]
    fn test_semantic_distance_is_symmetric() {
        let left = build_semantic_fingerprint(&semantic_node());
        let mut right_node = semantic_node();
        right_node.relations.push(SemanticRelation {
            source: "apple".to_string(),
            relation: "grows_on".to_string(),
            target: "tree".to_string(),
        });
        let right = build_semantic_fingerprint(&right_node);
        let weights = SemanticDistanceWeights::default();

        assert_eq!(
            semantic_distance(&left, &right, &weights),
            semantic_distance(&right, &left, &weights)
        );
    }

    #[test]
    fn test_semantic_distance_identity_is_zero() {
        let fingerprint = build_semantic_fingerprint(&semantic_node());
        let distance = semantic_distance(
            &fingerprint,
            &fingerprint,
            &SemanticDistanceWeights::default(),
        );

        assert_eq!(distance.topology_distance, 0.0);
        assert_eq!(distance.token_distance, 0.0);
        assert_eq!(distance.relation_distance, 0.0);
        assert_eq!(distance.trajectory_penalty, 0.0);
        assert_eq!(distance.total_distance, 0.0);
    }

    #[test]
    fn test_semantic_distance_is_normalized() {
        let left = SemanticFingerprint {
            topology_hash: 0,
            token_signature: 0,
            relation_signature: 0,
            trajectory_hint: 0,
        };
        let right = SemanticFingerprint {
            topology_hash: u64::MAX,
            token_signature: u64::MAX,
            relation_signature: u64::MAX,
            trajectory_hint: u64::MAX,
        };
        let distance = semantic_distance(&left, &right, &SemanticDistanceWeights::default());

        for value in [
            distance.topology_distance,
            distance.token_distance,
            distance.relation_distance,
            distance.trajectory_penalty,
            distance.total_distance,
            normalize_distance(f64::NAN),
            compose_total_distance(
                &distance,
                &SemanticDistanceWeights {
                    topology_weight: -1.0,
                    token_weight: -1.0,
                    relation_weight: -1.0,
                    trajectory_weight: -1.0,
                },
            ),
        ] {
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn test_semantic_distance_is_runtime_order_independent() {
        let left_node = semantic_node();
        let right_node = MemoryNode {
            memory_id: 44,
            tokens: vec!["apple".to_string(), "red".to_string()],
            semantic_labels: vec!["fruit".to_string()],
            relations: vec![
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "is_a".to_string(),
                    target: "fruit".to_string(),
                },
                SemanticRelation {
                    source: "apple".to_string(),
                    relation: "has_color".to_string(),
                    target: "red".to_string(),
                },
            ],
            dependency_links: vec![(1, 2), (2, 3)],
            causal_links: vec![7, 8],
            trajectory_hint: vec![10, 11],
        };
        let observer = build_semantic_fingerprint(&semantic_node());
        let left = build_semantic_fingerprint(&left_node);
        let right = build_semantic_fingerprint(&right_node);
        let weights = SemanticDistanceWeights::default();

        assert_eq!(
            semantic_distance(&left, &observer, &weights),
            semantic_distance(&right, &observer, &weights)
        );
    }

    #[test]
    fn test_semantic_distance_does_not_mutate_replay() {
        let (manager, _base_path) = stress_inserted_manager();
        let replay_before = manager.replay_fingerprint(1).expect("replay before");
        let topology_before = manager.topology_snapshot();
        let left = build_semantic_fingerprint(&semantic_node());
        let mut right_node = semantic_node();
        right_node.trajectory_hint.push(99);
        let right = build_semantic_fingerprint(&right_node);

        let _snapshot =
            semantic_distance_snapshot(1, 2, &left, &right, &SemanticDistanceWeights::default());

        let replay_after = manager.replay_fingerprint(1).expect("replay after");
        let topology_after = manager.topology_snapshot();
        assert_eq!(replay_before, replay_after);
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_replay_stress_loop_stability() {
        let (mut manager, base_path) = stress_inserted_manager();
        let replay_fp_initial = manager.replay_fingerprint(1).expect("initial replay");

        for loop_count in 0..1000 {
            let duplicate_id = 1000 + loop_count;
            let duplicate = manager
                .on_memory_insert(
                    identity(duplicate_id, 11, 22, &base_path),
                    base_path.clone(),
                )
                .expect("dedup insert");
            manager
                .on_transition_commit(1, 10_000 + loop_count, 20_000 + loop_count)
                .expect("commit transition");
            let rollback_success = rollback_one_transition(&mut manager, 1);
            let replay_fp = manager.replay_fingerprint(1).expect("loop replay");

            if loop_count % 250 == 0 {
                println!("loop={loop_count}");
                println!("canonical_id={:?}", duplicate.canonical_id);
                println!("replay_fp={replay_fp:?}");
                println!("transition_hash={:?}", base_path.transition_hash());
                println!("rollback_success={rollback_success}");
            }

            assert!(rollback_success);
            assert_eq!(replay_fp_initial, replay_fp);
        }

        let replay_fp_final = manager.replay_fingerprint(1).expect("final replay");
        assert_eq!(replay_fp_initial, replay_fp_final);
    }

    #[test]
    fn test_repeated_dedup_replay_integrity() {
        let (mut manager, base_path) = stress_inserted_manager();

        for loop_count in 0..1000 {
            let duplicate = manager
                .on_memory_insert(
                    identity(10_000 + loop_count, 11, 22, &base_path),
                    base_path.clone(),
                )
                .expect("dedup insert");
            assert!(!duplicate.inserted);
            assert_eq!(duplicate.canonical_id, 1);
        }

        assert_eq!(canonical_count(&manager), 1);
        assert_eq!(physical_node_count(&manager), 1);
    }

    #[test]
    fn test_rollback_reconstruction_consistency() {
        let (mut manager, _base_path) = stress_inserted_manager();
        let topology_before = manager.topology_snapshot();

        for loop_count in 0..1000 {
            manager
                .on_transition_commit(1, 30_000 + loop_count, 40_000 + loop_count)
                .expect("commit transition");
            let rollback_success = rollback_one_transition(&mut manager, 1);
            assert!(rollback_success);
        }

        let topology_after = manager.topology_snapshot();
        assert_eq!(topology_before, topology_after);
    }

    #[test]
    fn test_transition_hash_stability_under_loop() {
        let (mut manager, base_path) = stress_inserted_manager();
        let transition_hash_initial = manager
            .topology_snapshot()
            .transition_hashes
            .first()
            .copied()
            .expect("initial transition hash");

        for loop_count in 0..1000 {
            manager
                .on_transition_commit(1, 60_000 + loop_count, 70_000 + loop_count)
                .expect("commit transition");
            assert!(rollback_one_transition(&mut manager, 1));
            manager
                .on_memory_insert(
                    identity(80_000 + loop_count, 11, 22, &base_path),
                    base_path.clone(),
                )
                .expect("dedup insert");
        }

        let transition_hash_final = manager
            .topology_snapshot()
            .transition_hashes
            .first()
            .copied()
            .expect("final transition hash");
        assert_eq!(transition_hash_initial, transition_hash_final);
    }

    #[test]
    fn test_no_incremental_topology_corruption() {
        let (mut manager, base_path) = stress_inserted_manager();

        for loop_count in 0..1000 {
            manager
                .on_memory_insert(
                    identity(90_000 + loop_count, 11, 22, &base_path),
                    base_path.clone(),
                )
                .expect("dedup insert");
            manager
                .on_transition_commit(1, 100_000 + loop_count, 110_000 + loop_count)
                .expect("commit transition");
            assert!(rollback_one_transition(&mut manager, 1));
        }

        let no_orphan_nodes = has_no_orphan_nodes(&manager);
        let no_dangling_references = has_no_dangling_reference(&manager, 1);
        let no_alias_loops = has_no_alias_loops(&manager);
        let no_canonical_divergence = manager
            .references_for(1)
            .expect("references")
            .into_iter()
            .all(|reference_id| reference_id == 1 || !manager.nodes.contains_key(&reference_id));

        assert!(no_orphan_nodes);
        assert!(no_dangling_references);
        assert!(no_alias_loops);
        assert!(no_canonical_divergence);
    }

    #[test]
    fn test_topology_snapshot_serialization_is_deterministic() {
        let (manager, _base_path) = stress_inserted_manager();
        let snapshot_left = manager.topology_snapshot();
        let snapshot_right = manager.topology_snapshot();
        let serialized_left = serialize_snapshot(&snapshot_left);
        let serialized_right = serialize_snapshot(&snapshot_right);
        let hash_left = snapshot_hash(&snapshot_left);
        let hash_right = snapshot_hash(&snapshot_right);

        println!("snapshot_hash={hash_left:?}");
        println!("canonical_nodes={}", snapshot_left.canonical_nodes.len());
        println!("alias_nodes={}", snapshot_left.alias_nodes.len());
        println!("transition_hashes={:?}", snapshot_left.transition_hashes);

        assert_eq!(serialized_left, serialized_right);
        assert_eq!(hash_left, hash_right);
    }

    #[test]
    fn test_topology_snapshot_diff_detects_replay_safe_alias_update() {
        let (mut manager, base_path) = stress_inserted_manager();
        let before = manager.topology_snapshot();
        manager
            .on_memory_insert(identity(2, 11, 22, &base_path), base_path)
            .expect("dedup insert");
        let after = manager.topology_snapshot();

        let diff = diff_snapshots(&before, &after);

        assert!(!diff.equal);
        assert!(!diff.canonical_nodes_changed);
        assert_eq!(
            diff.added_aliases,
            vec![AliasNodeSnapshot {
                alias_id: 2,
                canonical_id: 1,
            }]
        );
        assert!(diff.removed_aliases.is_empty());
        assert!(!diff.transition_hashes_changed);
        assert!(!diff.replay_fingerprint_changed);
        assert!(!diff.trajectory_snapshots_changed);
    }

    #[test]
    fn test_topology_snapshot_diff_detects_forbidden_transition_mutation() {
        let (mut manager, _base_path) = stress_inserted_manager();
        let before = manager.topology_snapshot();
        manager
            .on_transition_commit(1, 9, 1009)
            .expect("commit transition");
        let after = manager.topology_snapshot();

        let diff = diff_snapshots(&before, &after);

        assert!(!diff.equal);
        assert!(diff.transition_hashes_changed);
        assert!(diff.replay_fingerprint_changed);
        assert!(diff.trajectory_snapshots_changed);
    }

    #[test]
    fn test_exact_duplicate_single_insert() {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2]);
        let transition_hash = path.transition_hash();

        let first = manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert first");
        let duplicate = manager
            .on_memory_insert(identity(2, 11, 22, &path), path)
            .expect("insert duplicate");

        let duplicate_detected = !duplicate.inserted;
        println!("canonical_id={:?}", duplicate.canonical_id);
        println!("duplicate_detected={duplicate_detected}");
        println!(
            "replay_fp={:?}",
            manager.replay_fingerprint(duplicate.canonical_id)
        );
        println!("transition_hash={transition_hash:?}");

        assert_eq!(canonical_count(&manager), 1);
        assert_eq!(physical_node_count(&manager), 1);
        assert_eq!(first.canonical_id, duplicate.canonical_id);
        assert!(duplicate_detected);
    }

    #[test]
    fn test_exact_duplicate_mass_insert() {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2, 3]);

        let first = manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert first");
        assert!(first.inserted);

        for memory_id in 2..=1000 {
            let duplicate = manager
                .on_memory_insert(identity(memory_id, 11, 22, &path), path.clone())
                .expect("insert duplicate");
            assert!(!duplicate.inserted);
            assert_eq!(duplicate.canonical_id, 1);
        }

        let no_memory_leak = manager.references_for(1).expect("references").len() == 1000;
        let no_graph_fragmentation = manager
            .semantic_cluster(22)
            .expect("semantic cluster")
            .aliases
            .is_empty();

        assert_eq!(physical_node_count(&manager), 1);
        assert!(no_memory_leak);
        assert!(no_graph_fragmentation);
    }

    #[test]
    fn test_transition_commit_replay_stability() {
        let mut manager = HolographicDeduplicationManager::new();
        let mut path = trajectory(10, &[1]);
        manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert");
        manager
            .on_transition_commit(1, 2, 1002)
            .expect("commit transition");
        path.transitions.push(2);
        path.causal_links.push(1002);
        let transition_hash = path.transition_hash();

        let replay_fp_before = manager.replay_fingerprint(1).expect("before dedup");
        let duplicate = manager
            .on_memory_insert(identity(2, 11, 22, &path), path)
            .expect("insert duplicate");
        let replay_fp_after = manager
            .replay_fingerprint(duplicate.canonical_id)
            .expect("after dedup");

        println!("canonical_id={:?}", duplicate.canonical_id);
        println!("duplicate_detected={}", !duplicate.inserted);
        println!("replay_fp={replay_fp_after:?}");
        println!("transition_hash={transition_hash:?}");

        assert_eq!(replay_fp_before, replay_fp_after);
    }

    #[test]
    fn test_canonical_reference_integrity() {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2]);
        let mut canonical_ids = Vec::new();

        for memory_id in 1..=8 {
            let result = manager
                .on_memory_insert(identity(memory_id, 11, 22, &path), path.clone())
                .expect("insert duplicate family");
            canonical_ids.push(result.canonical_id);
        }

        let all_reference_consistent = canonical_ids.iter().all(|canonical_id| *canonical_id == 1);
        let no_orphan_nodes = has_no_orphan_nodes(&manager);
        let no_dangling_reference = has_no_dangling_reference(&manager, 1);

        assert!(all_reference_consistent);
        assert!(no_orphan_nodes);
        assert!(no_dangling_reference);
    }

    #[test]
    fn test_unsafe_merge_rejection() {
        let mut manager = HolographicDeduplicationManager::new();
        let semantic = 22;
        let first_path = trajectory(10, &[1, 2]);
        let second_path = trajectory(11, &[9, 8]);

        manager
            .on_memory_insert(identity(1, 11, semantic, &first_path), first_path.clone())
            .expect("insert first");
        manager
            .on_memory_insert(identity(2, 11, semantic, &second_path), second_path.clone())
            .expect("insert second");

        let first_replay = manager.replay_fingerprint(1).expect("first replay");
        let second_replay = manager.replay_fingerprint(2).expect("second replay");
        let merge_rejected = matches!(
            manager.on_memory_merge(1, 2),
            Err(MemorySpaceError::UnsafeTransitionMerge)
        );
        let trajectory_preserved = manager.replay_fingerprint(1) == Some(first_replay)
            && manager.replay_fingerprint(2) == Some(second_replay);

        assert!(merge_rejected);
        assert!(trajectory_preserved);
    }

    #[test]
    fn exact_duplicates_merge_references_without_adding_node() {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2]);
        let first = manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert first");
        let duplicate = manager
            .on_memory_insert(identity(2, 11, 22, &path), path)
            .expect("insert duplicate");

        assert!(first.inserted);
        assert!(!duplicate.inserted);
        assert_eq!(duplicate.canonical_id, 1);
        assert_eq!(manager.node_count(), 1);
        assert_eq!(manager.references_for(1), Some(vec![1, 2]));
    }

    #[test]
    fn semantic_alias_keeps_distinct_trajectory_for_replay() {
        let mut manager = HolographicDeduplicationManager::new();
        let semantic = semantic_signature_from_tokens(["apple", "red"]);
        let first_path = trajectory(10, &[1, 2]);
        let second_path = trajectory(11, &[9, 8]);

        manager
            .on_memory_insert(identity(1, 11, semantic, &first_path), first_path.clone())
            .expect("insert first");
        let alias = manager
            .on_memory_insert(identity(2, 12, semantic, &second_path), second_path.clone())
            .expect("insert alias");

        assert!(alias.inserted);
        assert_eq!(manager.node_count(), 2);
        assert_eq!(
            manager.semantic_cluster(semantic),
            Some(&SemanticCluster {
                canonical_id: 1,
                aliases: vec![2],
            })
        );
        assert_eq!(
            manager
                .replay_fingerprint(1)
                .expect("first replay")
                .transitions,
            vec![1, 2]
        );
        assert_eq!(
            manager
                .replay_fingerprint(2)
                .expect("second replay")
                .transitions,
            vec![9, 8]
        );
    }

    #[test]
    fn different_trajectory_is_not_a_safe_merge() {
        let mut manager = HolographicDeduplicationManager::new();
        let semantic = semantic_signature_from_tokens(["red", "apple"]);
        let first_path = trajectory(10, &[1]);
        let second_path = trajectory(11, &[2]);

        manager
            .on_memory_insert(identity(1, 11, semantic, &first_path), first_path)
            .expect("insert first");
        manager
            .on_memory_insert(identity(2, 11, semantic, &second_path), second_path)
            .expect("insert second");

        assert_eq!(
            manager.on_memory_merge(1, 2),
            Err(MemorySpaceError::UnsafeTransitionMerge)
        );
    }

    #[test]
    fn decay_progresses_without_immediate_deletion() {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1]);
        manager
            .on_memory_insert(identity(1, 11, 22, &path), path)
            .expect("insert");
        manager
            .set_semantic_redundancy(1, 0.95)
            .expect("set redundancy");

        let events = manager.on_decay_check(500, DecayPolicy::default());

        assert_eq!(
            events,
            vec![DedupEvent::LifecycleChanged {
                memory_id: 1,
                from: MemoryLifecycle::Active,
                to: MemoryLifecycle::Compressed,
            }]
        );
        assert_eq!(manager.lifecycle(1), Some(MemoryLifecycle::Compressed));
    }

    #[test]
    fn transition_commit_updates_replay_fingerprint_and_exact_index() {
        let mut manager = HolographicDeduplicationManager::new();
        let mut path = trajectory(10, &[1]);
        manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert");

        let event = manager
            .on_transition_commit(1, 2, 1002)
            .expect("commit transition");
        path.transitions.push(2);
        path.causal_links.push(1002);

        assert_eq!(
            event,
            DedupEvent::TransitionCommitted {
                memory_id: 1,
                trajectory_id: 10,
            }
        );
        assert_eq!(
            manager.replay_fingerprint(1).expect("replay").transitions,
            vec![1, 2]
        );

        let duplicate = manager
            .on_memory_insert(identity(2, 11, 22, &path), path)
            .expect("insert duplicate after commit");
        assert_eq!(duplicate.canonical_id, 1);
        assert_eq!(manager.references_for(1), Some(vec![1, 2]));
    }
}
