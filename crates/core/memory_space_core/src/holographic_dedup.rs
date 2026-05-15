use std::collections::{BTreeMap, BTreeSet};

use crate::{MemoryId, MemorySpaceError};

pub type TransitionId = u64;
pub type TrajectoryId = u64;
pub type CausalLinkId = u64;
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

    fn stress_inserted_manager() -> (HolographicDeduplicationManager, StateTrajectory) {
        let mut manager = HolographicDeduplicationManager::new();
        let path = trajectory(10, &[1, 2]);
        manager
            .on_memory_insert(identity(1, 11, 22, &path), path.clone())
            .expect("insert canonical");
        (manager, path)
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
