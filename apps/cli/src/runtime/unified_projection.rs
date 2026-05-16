use crate::runtime::semantic::synchronize_semantic_projection;
use memory_space_core::{
    IdentityLineage, SemanticAttractor, SemanticDriftEvent, SemanticIdentityGraph,
    SemanticIdentitySnapshot, SemanticStabilizationState, detect_semantic_drift, identity_lineages,
    semantic_attractors, semantic_identity_snapshot, stabilization_state,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRuntimeSnapshot {
    pub state_label: String,
    pub target_label: Option<String>,
    pub active_transaction: bool,
    pub branch_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTransactionSnapshot {
    pub active_transaction_id: Option<String>,
    pub transaction_state: String,
    pub rollback_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticRuntimeSnapshot {
    pub topology_snapshot: SemanticIdentitySnapshot,
    pub attractor_snapshot: Vec<SemanticAttractor>,
    pub lineage_snapshot: Vec<IdentityLineage>,
    pub drift_snapshot: Vec<SemanticDriftEvent>,
    pub stabilization_snapshot: Vec<SemanticStabilizationState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedRuntimeSnapshot {
    pub runtime_revision: u64,
    pub execution_snapshot: ExecutionRuntimeSnapshot,
    pub semantic_snapshot: SemanticRuntimeSnapshot,
    pub transaction_snapshot: RuntimeTransactionSnapshot,
    pub projection_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedProjectionState {
    pub projection_revision: u64,
    pub replay_invariant: bool,
    pub topology_invariant: bool,
    pub synchronized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservabilityEventType {
    SemanticApply,
    Rollback,
    TopologyRewrite,
    ContradictionEscalation,
    AttractorCollapse,
    DriftRecovery,
    ProjectionSynchronization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeObservabilityEvent {
    pub event_id: u64,
    pub runtime_revision: u64,
    pub event_type: ObservabilityEventType,
    pub deterministic_timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Runtime {
    pub runtime_revision: u64,
    pub execution_snapshot: ExecutionRuntimeSnapshot,
    pub semantic_topology: SemanticIdentityGraph,
    pub transaction_snapshot: RuntimeTransactionSnapshot,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            runtime_revision: 0,
            execution_snapshot: ExecutionRuntimeSnapshot {
                state_label: "IDLE".to_string(),
                target_label: None,
                active_transaction: false,
                branch_depth: 0,
            },
            semantic_topology: SemanticIdentityGraph::default(),
            transaction_snapshot: RuntimeTransactionSnapshot {
                active_transaction_id: None,
                transaction_state: "none".to_string(),
                rollback_available: false,
            },
        }
    }
}

impl Runtime {
    pub fn increment_revision(&mut self) -> u64 {
        self.runtime_revision = self.runtime_revision.saturating_add(1);
        self.runtime_revision
    }
}

pub fn unified_runtime_snapshot(runtime: &Runtime) -> UnifiedRuntimeSnapshot {
    let semantic_snapshot = semantic_runtime_snapshot(&runtime.semantic_topology);
    let mut snapshot = UnifiedRuntimeSnapshot {
        runtime_revision: runtime.runtime_revision,
        execution_snapshot: runtime.execution_snapshot.clone(),
        semantic_snapshot,
        transaction_snapshot: runtime.transaction_snapshot.clone(),
        projection_checksum: 0,
    };
    snapshot.projection_checksum = deterministic_projection_checksum(&snapshot);
    snapshot
}

pub fn semantic_runtime_snapshot(topology: &SemanticIdentityGraph) -> SemanticRuntimeSnapshot {
    let mut normalized = topology.clone();
    normalized
        .identities
        .sort_by(|left, right| left.identity_id.cmp(&right.identity_id));
    for identity in &mut normalized.identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
    }

    let topology_snapshot = semantic_identity_snapshot(0, &normalized);
    let previous_snapshot = SemanticIdentitySnapshot {
        timestamp: 0,
        identities: Vec::new(),
    };
    let mut attractor_snapshot = semantic_attractors(&normalized);
    let mut lineage_snapshot = identity_lineages(&normalized);
    let mut drift_snapshot = detect_semantic_drift(&previous_snapshot, &topology_snapshot);
    let mut stabilization_snapshot = normalized
        .identities
        .iter()
        .map(stabilization_state)
        .collect::<Vec<_>>();

    attractor_snapshot.sort_by(|left, right| left.attractor_id.cmp(&right.attractor_id));
    lineage_snapshot.sort_by(|left, right| left.ancestor_id.cmp(&right.ancestor_id));
    drift_snapshot.sort_by(|left, right| left.identity_id.cmp(&right.identity_id));
    stabilization_snapshot.sort_by(|left, right| left.identity_id.cmp(&right.identity_id));

    SemanticRuntimeSnapshot {
        topology_snapshot,
        attractor_snapshot,
        lineage_snapshot,
        drift_snapshot,
        stabilization_snapshot,
    }
}

pub fn synchronize_unified_projection(snapshot: &UnifiedRuntimeSnapshot) -> UnifiedProjectionState {
    let expected_checksum = deterministic_projection_checksum(snapshot);
    let synchronized = expected_checksum == snapshot.projection_checksum;
    let semantic_projection = synchronize_semantic_projection(&SemanticIdentityGraph {
        identities: snapshot
            .semantic_snapshot
            .topology_snapshot
            .identities
            .clone(),
    });

    UnifiedProjectionState {
        projection_revision: stable_hash_u64s([
            snapshot.runtime_revision,
            expected_checksum,
            semantic_projection.topology_revision,
        ]),
        replay_invariant: projection_replay_validation(snapshot),
        topology_invariant: semantic_projection.topology_revision != 0
            || snapshot
                .semantic_snapshot
                .topology_snapshot
                .identities
                .is_empty(),
        synchronized,
    }
}

pub fn deterministic_projection_checksum(snapshot: &UnifiedRuntimeSnapshot) -> u64 {
    let mut hash = StableHasher::new();
    hash.write_u64(snapshot.runtime_revision);
    hash.write_str(&snapshot.execution_snapshot.state_label);
    hash.write_opt_str(snapshot.execution_snapshot.target_label.as_deref());
    hash.write_bool(snapshot.execution_snapshot.active_transaction);
    hash.write_u64(snapshot.execution_snapshot.branch_depth as u64);
    hash.write_opt_str(
        snapshot
            .transaction_snapshot
            .active_transaction_id
            .as_deref(),
    );
    hash.write_str(&snapshot.transaction_snapshot.transaction_state);
    hash.write_bool(snapshot.transaction_snapshot.rollback_available);

    let semantic = &snapshot.semantic_snapshot;
    hash.write_u64(semantic.topology_snapshot.timestamp);
    for identity in &semantic.topology_snapshot.identities {
        hash.write_u64(identity.identity_id);
        hash.write_u64(identity.continuity_score.to_bits());
        hash.write_u64(identity.invariant_core_overlap.to_bits());
        for lineage in &identity.drift_lineage {
            hash.write_u64(*lineage);
        }
    }
    for attractor in &semantic.attractor_snapshot {
        hash.write_u64(attractor.attractor_id);
        hash.write_u64(attractor.invariant_density.to_bits());
        hash.write_u64(attractor.stability_score.to_bits());
        hash.write_u64(attractor.attractor_strength.to_bits());
        hash.write_u64(attractor.semantic_mass.to_bits());
        for anchor in &attractor.anchor_set {
            hash.write_u64(anchor.anchor_id);
            hash.write_u64(anchor.identity_id);
            hash.write_u64(anchor.invariant_core.core_id);
        }
    }
    for lineage in &semantic.lineage_snapshot {
        hash.write_u64(lineage.ancestor_id);
        for descendant in &lineage.descendant_ids {
            hash.write_u64(*descendant);
        }
    }
    for drift in &semantic.drift_snapshot {
        hash.write_u64(drift.identity_id);
        hash.write_u64(drift.previous_continuity.to_bits());
        hash.write_u64(drift.current_continuity.to_bits());
        hash.write_u64(drift.drift_magnitude.to_bits());
        hash.write_bool(drift.recoverable);
    }
    for state in &semantic.stabilization_snapshot {
        hash.write_u64(state.identity_id);
        hash.write_u64(state.attractor_id);
        hash.write_u64(state.continuity_score.to_bits());
        hash.write_u64(state.drift_score.to_bits());
        hash.write_u64(state.contradiction_density.to_bits());
        hash.write_u64(state.stabilization_confidence.to_bits());
        hash.write_bool(state.recoverable);
    }
    hash.finish()
}

pub fn projection_replay_validation(snapshot: &UnifiedRuntimeSnapshot) -> bool {
    let replay = UnifiedRuntimeSnapshot {
        runtime_revision: snapshot.runtime_revision,
        execution_snapshot: snapshot.execution_snapshot.clone(),
        semantic_snapshot: semantic_runtime_snapshot(&SemanticIdentityGraph {
            identities: snapshot
                .semantic_snapshot
                .topology_snapshot
                .identities
                .clone(),
        }),
        transaction_snapshot: snapshot.transaction_snapshot.clone(),
        projection_checksum: 0,
    };
    deterministic_projection_checksum(&replay) == deterministic_projection_checksum(snapshot)
}

pub fn deterministic_observability_events(
    snapshot: &UnifiedRuntimeSnapshot,
) -> Vec<RuntimeObservabilityEvent> {
    let mut event_types = Vec::new();
    event_types.push(ObservabilityEventType::ProjectionSynchronization);
    if snapshot.transaction_snapshot.rollback_available {
        event_types.push(ObservabilityEventType::Rollback);
    }
    if !snapshot
        .semantic_snapshot
        .topology_snapshot
        .identities
        .is_empty()
    {
        event_types.push(ObservabilityEventType::SemanticApply);
        event_types.push(ObservabilityEventType::TopologyRewrite);
    }
    if snapshot
        .semantic_snapshot
        .attractor_snapshot
        .iter()
        .any(|attractor| attractor.attractor_strength < 0.50)
    {
        event_types.push(ObservabilityEventType::AttractorCollapse);
    }
    if snapshot
        .semantic_snapshot
        .drift_snapshot
        .iter()
        .any(|drift| drift.recoverable)
    {
        event_types.push(ObservabilityEventType::DriftRecovery);
    }
    event_types.sort();
    event_types.dedup();

    event_types
        .into_iter()
        .enumerate()
        .map(|(index, event_type)| RuntimeObservabilityEvent {
            event_id: stable_hash_u64s([
                snapshot.runtime_revision,
                index as u64,
                event_type as u64,
            ]),
            runtime_revision: snapshot.runtime_revision,
            event_type,
            deterministic_timestamp: snapshot
                .runtime_revision
                .saturating_mul(1_000)
                .saturating_add(index as u64),
        })
        .collect()
}

pub fn render_unified_snapshot(snapshot: &UnifiedRuntimeSnapshot) -> String {
    let projection = synchronize_unified_projection(snapshot);
    format!(
        "runtime_revision: {}\nprojection_checksum: {}\nprojection_revision: {}\nreplay_invariant: {}\ntopology_invariant: {}\nsynchronized: {}\nexecution_state: {}\ntarget: {}\ntransaction_state: {}\nsemantic_identities: {}\nattractors: {}\nlineages: {}\ndrift_events: {}\nstabilization_states: {}",
        snapshot.runtime_revision,
        snapshot.projection_checksum,
        projection.projection_revision,
        projection.replay_invariant,
        projection.topology_invariant,
        projection.synchronized,
        snapshot.execution_snapshot.state_label,
        snapshot
            .execution_snapshot
            .target_label
            .as_deref()
            .unwrap_or("(none)"),
        snapshot.transaction_snapshot.transaction_state,
        snapshot
            .semantic_snapshot
            .topology_snapshot
            .identities
            .len(),
        snapshot.semantic_snapshot.attractor_snapshot.len(),
        snapshot.semantic_snapshot.lineage_snapshot.len(),
        snapshot.semantic_snapshot.drift_snapshot.len(),
        snapshot.semantic_snapshot.stabilization_snapshot.len(),
    )
}

pub fn render_revision(snapshot: &UnifiedRuntimeSnapshot) -> String {
    let projection = synchronize_unified_projection(snapshot);
    format!(
        "runtime_revision: {}\nprojection_revision: {}\nchecksum: {}\nreplay_invariant: {}\nsynchronized: {}",
        snapshot.runtime_revision,
        projection.projection_revision,
        snapshot.projection_checksum,
        projection.replay_invariant,
        projection.synchronized,
    )
}

fn stable_hash_u64s(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut hasher = StableHasher::new();
    for value in values {
        hasher.write_u64(value);
    }
    hasher.finish()
}

struct StableHasher {
    hash: u64,
}

impl StableHasher {
    fn new() -> Self {
        Self {
            hash: 0xcbf29ce484222325_u64,
        }
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.hash ^= u64::from(byte);
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        for byte in value.as_bytes() {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
    }

    fn write_opt_str(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.write_bool(true);
                self.write_str(value);
            }
            None => self.write_bool(false),
        }
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u64(u64::from(value));
    }

    fn finish(self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_core::SemanticIdentityCandidate;

    fn runtime_with_semantics() -> Runtime {
        Runtime {
            runtime_revision: 3,
            execution_snapshot: ExecutionRuntimeSnapshot {
                state_label: "PREVIEW_READY".to_string(),
                target_label: Some("apps/cli/src/core.rs".to_string()),
                active_transaction: true,
                branch_depth: 1,
            },
            semantic_topology: SemanticIdentityGraph {
                identities: vec![
                    SemanticIdentityCandidate {
                        identity_id: 2,
                        continuity_score: 0.82,
                        invariant_core_overlap: 0.92,
                        drift_lineage: vec![20, 10, 10],
                    },
                    SemanticIdentityCandidate {
                        identity_id: 1,
                        continuity_score: 0.90,
                        invariant_core_overlap: 0.95,
                        drift_lineage: vec![5],
                    },
                ],
            },
            transaction_snapshot: RuntimeTransactionSnapshot {
                active_transaction_id: Some("tx-1".to_string()),
                transaction_state: "active".to_string(),
                rollback_available: true,
            },
        }
    }

    #[test]
    fn unified_snapshot_generation_is_deterministic() {
        let snapshot = unified_runtime_snapshot(&runtime_with_semantics());
        let replay = unified_runtime_snapshot(&runtime_with_semantics());

        assert_eq!(snapshot.projection_checksum, replay.projection_checksum);
        assert_eq!(
            snapshot.semantic_snapshot.topology_snapshot.identities[0].identity_id,
            1
        );
        assert!(projection_replay_validation(&snapshot));
    }

    #[test]
    fn stale_projection_is_rejected() {
        let mut snapshot = unified_runtime_snapshot(&runtime_with_semantics());
        snapshot.projection_checksum ^= 0x55;

        let state = synchronize_unified_projection(&snapshot);

        assert!(!state.synchronized);
        assert!(state.replay_invariant);
    }

    #[test]
    fn revision_increment_is_unified() {
        let mut runtime = Runtime::default();

        assert_eq!(runtime.increment_revision(), 1);
        assert_eq!(runtime.increment_revision(), 2);
    }

    #[test]
    fn observability_event_ordering_is_deterministic() {
        let snapshot = unified_runtime_snapshot(&runtime_with_semantics());
        let first = deterministic_observability_events(&snapshot);
        let second = deterministic_observability_events(&snapshot);

        assert_eq!(first, second);
        assert!(
            first
                .windows(2)
                .all(|pair| pair[0].event_type <= pair[1].event_type)
        );
    }
}
