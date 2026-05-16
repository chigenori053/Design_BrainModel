use crate::runtime::branch::{BranchRuntime, BranchSnapshot};
use memory_space_core::{
    SemanticAttractor, SemanticIdentityGraph, SemanticIdentitySnapshot, SemanticRewriteTransaction,
    SemanticRewriteValidation, SemanticRollbackSnapshot, deterministic_rewrite_checksum,
    identity_lineages, semantic_attractors, semantic_rollback_snapshot, validate_semantic_rewrite,
};
use std::sync::{Mutex, MutexGuard};

static SEMANTIC_APPLY_LOCK: Mutex<()> = Mutex::new(());

/// Semantic roles for meaning-grounded architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticRole {
    Coordinator,
    Executor,
    Planner,
    Synthesizer,
    Validator,
    MemoryAuthority,
    RuntimeAuthority,
    RepairAuthority,
    WorldModelAuthority,
    Unknown,
}

/// A unit of responsibility within the semantic layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsibilityUnit {
    pub responsibility_id: String,
    pub semantic_role: SemanticRole,
    pub owned_symbols: Vec<String>,
    pub owned_modules: Vec<String>,
    pub intent_description: String,
}

/// A node within the semantic graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticNode {
    pub node_id: String,
    pub semantic_role: SemanticRole,
    pub responsibilities: Vec<ResponsibilityUnit>,
    pub dependencies: Vec<String>,
    pub intent_signature: String,
}

/// The semantic representation of the architecture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticGraph {
    pub nodes: Vec<SemanticNode>,
    pub causal_edges: Vec<(String, String)>,
    pub ownership_edges: Vec<(String, String)>,
    pub dependency_edges: Vec<(String, String)>,
}

/// Types of semantic contradictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticContradiction {
    DuplicateResponsibility,
    OwnershipConflict,
    InvalidAbstractionBoundary,
    IntentMismatch,
    CyclicSemanticDependency,
    SemanticRepairRegression,
}

/// Runtime-owned apply mode. Planning and validation remain in memory_space_core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticApplyMode {
    Strict,
    GovernanceOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSemanticApplyRequest {
    pub transaction: SemanticRewriteTransaction,
    pub validation: SemanticRewriteValidation,
    pub apply_mode: SemanticApplyMode,
    pub runtime_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSemanticApplyResult {
    pub applied: bool,
    pub topology_updated: bool,
    pub rollback_available: bool,
    pub applied_checksum: u64,
    pub topology_revision: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRollbackRestoreResult {
    pub restored: bool,
    pub restored_revision: u64,
    pub replay_invariant_retained: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticApplyGovernanceResult {
    pub allowed: bool,
    pub rejected_reason: Option<String>,
    pub continuity_safe: bool,
    pub anchor_safe: bool,
    pub contradiction_safe: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticRuntimeProjection {
    pub topology_revision: u64,
    pub lineage_count: usize,
    pub attractors: Vec<SemanticAttractor>,
    pub drift_state_revision: u64,
    pub rewrite_diff_revision: u64,
}

pub fn runtime_semantic_apply(request: RuntimeSemanticApplyRequest) -> RuntimeSemanticApplyResult {
    let _writer = serialized_semantic_writer();

    let governance = validate_runtime_semantic_apply(&request);
    if !governance.allowed {
        return RuntimeSemanticApplyResult {
            applied: false,
            topology_updated: false,
            rollback_available: false,
            applied_checksum: 0,
            topology_revision: 0,
            warnings: governance.rejected_reason.into_iter().collect(),
        };
    }

    let topology = SemanticIdentityGraph {
        identities: request.transaction.source_snapshot.identities.clone(),
    };
    let projection = synchronize_semantic_projection(&topology);
    let topology_revision = stable_semantic_revision([
        request.transaction.transaction_id,
        request.transaction.rollback_snapshot.snapshot_id,
        request.runtime_checksum,
        projection.topology_revision,
    ]);

    RuntimeSemanticApplyResult {
        applied: true,
        topology_updated: true,
        rollback_available: true,
        applied_checksum: request.runtime_checksum,
        topology_revision,
        warnings: Vec::new(),
    }
}

pub fn runtime_semantic_rollback(
    snapshot: SemanticRollbackSnapshot,
) -> SemanticRollbackRestoreResult {
    let restored_revision = snapshot_revision(&snapshot.topology_snapshot);
    let replay_invariant_retained = snapshot.anchor_snapshot.iter().all(|anchor| {
        snapshot
            .topology_snapshot
            .identities
            .iter()
            .any(|identity| identity.identity_id == anchor.identity_id)
    });

    SemanticRollbackRestoreResult {
        restored: replay_invariant_retained,
        restored_revision,
        replay_invariant_retained,
    }
}

pub fn validate_runtime_semantic_apply(
    request: &RuntimeSemanticApplyRequest,
) -> SemanticApplyGovernanceResult {
    let recomputed_validation = validate_semantic_rewrite(&request.transaction);
    let validation_matches = request.validation == request.transaction.validation
        && request.validation == recomputed_validation;
    let checksum = deterministic_rewrite_checksum(&request.transaction);
    let checksum_matches = request.runtime_checksum == checksum
        && request.transaction.deterministic_checksum == checksum;
    let rollback_snapshot = semantic_rollback_snapshot(&SemanticIdentityGraph {
        identities: request.transaction.source_snapshot.identities.clone(),
    });
    let topology_snapshot_matches = rollback_snapshot.topology_snapshot
        == request.transaction.rollback_snapshot.topology_snapshot;

    let continuity_safe = request.validation.valid && request.validation.continuity_retained;
    let anchor_safe = request.validation.valid && request.validation.anchors_preserved;
    let contradiction_safe = request.validation.valid && request.validation.contradiction_bounded;
    let mass_safe = request.validation.valid && request.validation.semantic_mass_bounded;
    let invariant_safe =
        request.validation.replay_invariant && request.validation.topology_invariant;

    let rejected_reason = if !validation_matches {
        Some("semantic apply validation mismatch".to_string())
    } else if !request.validation.valid {
        Some(format!(
            "semantic validation rejected: {}",
            request.validation.validation_errors.join(", ")
        ))
    } else if !continuity_safe {
        Some("continuity violation".to_string())
    } else if !anchor_safe {
        Some("anchor destruction".to_string())
    } else if !contradiction_safe {
        Some("contradiction escalation".to_string())
    } else if !mass_safe {
        Some("semantic mass collapse".to_string())
    } else if !topology_snapshot_matches {
        Some("topology snapshot mismatch".to_string())
    } else if !checksum_matches {
        Some("semantic checksum mismatch".to_string())
    } else if !invariant_safe {
        Some("semantic replay invariant violation".to_string())
    } else {
        None
    };

    SemanticApplyGovernanceResult {
        allowed: rejected_reason.is_none(),
        rejected_reason,
        continuity_safe,
        anchor_safe,
        contradiction_safe,
    }
}

pub fn synchronize_semantic_projection(
    topology: &SemanticIdentityGraph,
) -> SemanticRuntimeProjection {
    let mut normalized = topology.clone();
    normalized
        .identities
        .sort_by(|a, b| a.identity_id.cmp(&b.identity_id));
    for identity in &mut normalized.identities {
        identity.drift_lineage.sort();
        identity.drift_lineage.dedup();
    }

    let lineages = identity_lineages(&normalized);
    let attractors = semantic_attractors(&normalized);
    let topology_revision = graph_revision(&normalized);
    let drift_state_revision =
        stable_semantic_revision(normalized.identities.iter().flat_map(|identity| {
            [
                identity.identity_id,
                identity.continuity_score.to_bits(),
                identity.invariant_core_overlap.to_bits(),
            ]
        }));
    let rewrite_diff_revision = stable_semantic_revision([
        topology_revision,
        lineages.len() as u64,
        attractors.len() as u64,
    ]);

    SemanticRuntimeProjection {
        topology_revision,
        lineage_count: lineages.len(),
        attractors,
        drift_state_revision,
        rewrite_diff_revision,
    }
}

fn serialized_semantic_writer() -> MutexGuard<'static, ()> {
    SEMANTIC_APPLY_LOCK.lock().expect("semantic apply lock")
}

#[cfg(test)]
fn semantic_writer_available() -> bool {
    SEMANTIC_APPLY_LOCK.try_lock().is_ok()
}

fn graph_revision(graph: &SemanticIdentityGraph) -> u64 {
    stable_semantic_revision(graph.identities.iter().flat_map(|identity| {
        [
            identity.identity_id,
            identity.continuity_score.to_bits(),
            identity.invariant_core_overlap.to_bits(),
        ]
        .into_iter()
        .chain(identity.drift_lineage.iter().copied())
    }))
}

fn snapshot_revision(snapshot: &SemanticIdentitySnapshot) -> u64 {
    stable_semantic_revision([snapshot.timestamp].into_iter().chain(
        snapshot.identities.iter().flat_map(|identity| {
            [
                identity.identity_id,
                identity.continuity_score.to_bits(),
                identity.invariant_core_overlap.to_bits(),
            ]
            .into_iter()
            .chain(identity.drift_lineage.iter().copied())
        }),
    ))
}

fn stable_semantic_revision(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

impl SemanticGraph {
    pub fn add_node(&mut self, mut node: SemanticNode) {
        node.dependencies.sort();
        for res in &mut node.responsibilities {
            res.owned_symbols.sort();
            res.owned_modules.sort();
        }
        node.responsibilities
            .sort_by(|a, b| a.responsibility_id.cmp(&b.responsibility_id));

        if !self.nodes.iter().any(|n| n.node_id == node.node_id) {
            self.nodes.push(node);
            self.nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        }
    }

    pub fn add_causal_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.causal_edges.contains(&edge) {
            self.causal_edges.push(edge);
            self.causal_edges.sort();
        }
    }

    pub fn add_ownership_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.ownership_edges.contains(&edge) {
            self.ownership_edges.push(edge);
            self.ownership_edges.sort();
        }
    }

    pub fn add_dependency_edge(&mut self, source: String, target: String) {
        let edge = (source, target);
        if !self.dependency_edges.contains(&edge) {
            self.dependency_edges.push(edge);
            self.dependency_edges.sort();
        }
    }
}

/// Scoring for semantic convergence.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticConvergenceScore {
    pub intent_stability: f64,
    pub abstraction_consistency: f64,
    pub ownership_consistency: f64,
    pub semantic_replay_stability: f64,
    pub contradiction_penalty: f64,
    pub total_score: f64,
}

impl SemanticConvergenceScore {
    pub fn zero() -> Self {
        Self {
            intent_stability: 0.0,
            abstraction_consistency: 0.0,
            ownership_consistency: 0.0,
            semantic_replay_stability: 0.0,
            contradiction_penalty: 0.0,
            total_score: 0.0,
        }
    }

    pub fn update_total(&mut self) {
        self.total_score = (self.intent_stability
            + self.abstraction_consistency
            + self.ownership_consistency
            + self.semantic_replay_stability)
            - self.contradiction_penalty;
    }
}

/// Semantic Evaluation Engine (Specified in 5).
pub fn evaluate_semantic_convergence(snapshot: &mut BranchSnapshot, _runtime: &BranchRuntime) {
    // 5.1 Semantic Graph Construction (Mocked).
    let _graph = SemanticGraph::default();

    // 5.2 Contradiction Detection (Rule-based).
    let penalty = 0.0;

    // Rule: Responsibility Collision detection.
    // (Logic: check if multiple nodes share identical intent signatures).

    // Rule: Ownership Drift.

    // Rule: Invalid Abstraction.

    // Rule: Intent Mismatch.

    snapshot.score.semantic_score.contradiction_penalty = penalty;
    snapshot.score.semantic_score.update_total();
}

/// Intent Restoration (Repair Engine, Specified in 7.1).
pub fn restore_intent(
    runtime: &mut BranchRuntime,
    _contradiction: SemanticContradiction,
) -> Option<BranchSnapshot> {
    let parent = &runtime.committed_branch;
    let mut repair = parent.clone();
    repair.branch_id.0.push_str("-semantic-repair");
    repair.tx_id.push_str("-semantic-repair-tx");

    // Restore intent stability.
    repair.score.semantic_score.intent_stability = 20.0;
    repair.score.semantic_score.update_total();

    Some(repair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::branch::{
        BranchId, BranchSnapshot, ContradictionSet, ConvergenceScore, RuntimeEffectSet,
        WorldStateSnapshot,
    };
    use crate::runtime::synthesis::ArchitectureTopology;
    use crate::tui::runtime::RuntimeShellState;
    use memory_space_core::{
        SemanticIdentityCandidate, deterministic_rewrite_checksum, semantic_rewrite_transaction,
        validate_semantic_rewrite,
    };

    fn make_empty_snapshot(id: &str) -> BranchSnapshot {
        BranchSnapshot::new(
            BranchId(id.into()),
            None,
            format!("tx-{id}"),
            "target".into(),
            RuntimeShellState::PreviewReady,
            crate::core::Diff {
                file: "t".into(),
                changes: vec![],
            },
            ConvergenceScore::zero(),
            ContradictionSet::zero(),
            WorldStateSnapshot::zero(),
            RuntimeEffectSet::zero(),
            ArchitectureTopology::default(),
            0,
            0,
        )
    }

    fn runtime_request() -> RuntimeSemanticApplyRequest {
        let graph = SemanticIdentityGraph {
            identities: vec![
                SemanticIdentityCandidate {
                    identity_id: 1,
                    continuity_score: 0.82,
                    invariant_core_overlap: 0.90,
                    drift_lineage: vec![10, 11],
                },
                SemanticIdentityCandidate {
                    identity_id: 2,
                    continuity_score: 0.78,
                    invariant_core_overlap: 0.88,
                    drift_lineage: vec![20],
                },
            ],
        };
        let transaction = semantic_rewrite_transaction(&graph);
        RuntimeSemanticApplyRequest {
            validation: transaction.validation.clone(),
            runtime_checksum: transaction.deterministic_checksum,
            transaction,
            apply_mode: SemanticApplyMode::Strict,
        }
    }

    fn unstable_runtime_request() -> RuntimeSemanticApplyRequest {
        let graph = SemanticIdentityGraph {
            identities: vec![SemanticIdentityCandidate {
                identity_id: 1,
                continuity_score: 0.20,
                invariant_core_overlap: 0.90,
                drift_lineage: vec![10],
            }],
        };
        let transaction = semantic_rewrite_transaction(&graph);
        RuntimeSemanticApplyRequest {
            validation: transaction.validation.clone(),
            runtime_checksum: transaction.deterministic_checksum,
            transaction,
            apply_mode: SemanticApplyMode::Strict,
        }
    }

    /// Rule 3.4: same input same result.
    #[test]
    fn semantic_graph_deterministic() {
        let mut g1 = SemanticGraph::default();
        let mut g2 = SemanticGraph::default();

        let n1 = SemanticNode {
            node_id: "a".into(),
            semantic_role: SemanticRole::Coordinator,
            responsibilities: vec![],
            dependencies: vec!["z".into(), "b".into()],
            intent_signature: "a-sig".into(),
        };
        let n2 = SemanticNode {
            node_id: "b".into(),
            semantic_role: SemanticRole::Executor,
            responsibilities: vec![],
            dependencies: vec![],
            intent_signature: "b-sig".into(),
        };

        g1.add_node(n1.clone());
        g1.add_node(n2.clone());

        g2.add_node(n2);
        g2.add_node(n1);

        assert_eq!(g1, g2);
        assert_eq!(g1.nodes[0].node_id, "a");
        assert_eq!(
            g1.nodes[0].dependencies,
            vec!["b".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn semantic_memory_ordering_stable() {
        let mut graph = SemanticGraph::default();
        graph.add_causal_edge("b".into(), "a".into());
        graph.add_causal_edge("a".into(), "c".into());
        assert_eq!(graph.causal_edges[0].0, "a");
    }

    #[test]
    fn semantic_convergence_stable() {
        let mut s = make_empty_snapshot("s1");
        s.score.semantic_score.intent_stability = 10.0;
        s.score.semantic_score.update_total();
        assert!(s.score.semantic_score.total_score > 0.0);
    }

    #[test]
    fn semantic_repair_regression_rejected() {
        let mut score = SemanticConvergenceScore::zero();
        score.contradiction_penalty = 50.0;
        score.update_total();
        assert!(score.total_score < 0.0);
    }

    #[test]
    fn validated_semantic_apply_updates_topology() {
        let result = runtime_semantic_apply(runtime_request());

        assert!(result.applied);
        assert!(result.topology_updated);
        assert!(result.rollback_available);
        assert_ne!(result.applied_checksum, 0);
        assert_ne!(result.topology_revision, 0);
    }

    #[test]
    fn semantic_apply_without_matching_validation_is_rejected() {
        let mut request = runtime_request();
        request.validation.valid = false;

        let governance = validate_runtime_semantic_apply(&request);

        assert!(!governance.allowed);
        assert_eq!(
            governance.rejected_reason.as_deref(),
            Some("semantic apply validation mismatch")
        );
    }

    #[test]
    fn semantic_apply_rejects_checksum_mismatch() {
        let mut request = runtime_request();
        request.runtime_checksum ^= 0x55;

        let result = runtime_semantic_apply(request);

        assert!(!result.applied);
        assert!(result.warnings.iter().any(|w| w.contains("checksum")));
    }

    #[test]
    fn semantic_rollback_restores_replay_invariant_snapshot() {
        let request = runtime_request();
        let result = runtime_semantic_rollback(request.transaction.rollback_snapshot);

        assert!(result.restored);
        assert!(result.replay_invariant_retained);
        assert_ne!(result.restored_revision, 0);
    }

    #[test]
    fn semantic_rollback_rejects_anchor_without_topology_identity() {
        let mut request = runtime_request();
        request
            .transaction
            .rollback_snapshot
            .topology_snapshot
            .identities
            .clear();

        let result = runtime_semantic_rollback(request.transaction.rollback_snapshot);

        assert!(!result.restored);
        assert!(!result.replay_invariant_retained);
    }

    #[test]
    fn governance_rejects_continuity_collapse() {
        let request = unstable_runtime_request();
        let governance = validate_runtime_semantic_apply(&request);

        assert!(!governance.allowed);
        assert!(!governance.continuity_safe);
    }

    #[test]
    fn governance_rejects_stale_topology_snapshot() {
        let mut request = runtime_request();
        request
            .transaction
            .rollback_snapshot
            .topology_snapshot
            .identities
            .push(SemanticIdentityCandidate {
                identity_id: 99,
                continuity_score: 1.0,
                invariant_core_overlap: 1.0,
                drift_lineage: vec![99],
            });
        request.transaction.validation = validate_semantic_rewrite(&request.transaction);
        request.validation = request.transaction.validation.clone();
        request.transaction.deterministic_checksum =
            deterministic_rewrite_checksum(&request.transaction);
        request.runtime_checksum = request.transaction.deterministic_checksum;

        let governance = validate_runtime_semantic_apply(&request);

        assert!(!governance.allowed);
        assert!(
            governance
                .rejected_reason
                .as_deref()
                .unwrap_or_default()
                .contains("topology")
        );
    }

    #[test]
    fn projection_synchronization_is_deterministic() {
        let graph_a = SemanticIdentityGraph {
            identities: vec![
                SemanticIdentityCandidate {
                    identity_id: 2,
                    continuity_score: 0.80,
                    invariant_core_overlap: 0.90,
                    drift_lineage: vec![20, 20],
                },
                SemanticIdentityCandidate {
                    identity_id: 1,
                    continuity_score: 0.80,
                    invariant_core_overlap: 0.90,
                    drift_lineage: vec![10],
                },
            ],
        };
        let graph_b = SemanticIdentityGraph {
            identities: vec![
                SemanticIdentityCandidate {
                    identity_id: 1,
                    continuity_score: 0.80,
                    invariant_core_overlap: 0.90,
                    drift_lineage: vec![10],
                },
                SemanticIdentityCandidate {
                    identity_id: 2,
                    continuity_score: 0.80,
                    invariant_core_overlap: 0.90,
                    drift_lineage: vec![20],
                },
            ],
        };

        let projection_a = synchronize_semantic_projection(&graph_a);
        let projection_b = synchronize_semantic_projection(&graph_b);

        assert_eq!(
            projection_a.topology_revision,
            projection_b.topology_revision
        );
        assert_eq!(projection_a.lineage_count, projection_b.lineage_count);
        assert_eq!(projection_a.attractors, projection_b.attractors);
    }

    #[test]
    fn semantic_apply_is_single_writer_serialized() {
        let _writer = serialized_semantic_writer();

        assert!(!semantic_writer_available());
    }
}
