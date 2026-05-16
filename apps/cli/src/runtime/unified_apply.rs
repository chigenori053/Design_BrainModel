use std::sync::{Mutex, TryLockError};

use crate::runtime::semantic::{
    RuntimeSemanticApplyRequest, SemanticApplyMode, runtime_semantic_apply,
    runtime_semantic_rollback, validate_runtime_semantic_apply,
};
use crate::runtime::unified_projection::{
    ExecutionRuntimeSnapshot, Runtime, RuntimeTransactionSnapshot, synchronize_unified_projection,
    unified_runtime_snapshot,
};
use memory_space_core::{
    SemanticIdentityGraph, SemanticRewriteTransaction, SemanticRollbackSnapshot,
    SemanticTopologyDiff, deterministic_rewrite_checksum, semantic_rollback_snapshot,
    validate_semantic_rewrite,
};

static UNIFIED_RUNTIME_WRITER: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDiff {
    pub target: String,
    pub operation_count: usize,
    pub deterministic_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRollbackSnapshot {
    pub execution_snapshot: ExecutionRuntimeSnapshot,
    pub transaction_snapshot: RuntimeTransactionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionApplyTransaction {
    pub transaction_id: u64,
    pub diff: ExecutionDiff,
    pub rollback_snapshot: ExecutionRollbackSnapshot,
    pub execution_revision: u64,
    pub deterministic_checksum: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStateDelta {
    pub revision_before: u64,
    pub revision_after: u64,
    pub execution_changed: bool,
    pub semantic_changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedMutationPreview {
    pub execution_diff: Option<ExecutionDiff>,
    pub topology_diff: Option<SemanticTopologyDiff>,
    pub continuity_delta: f64,
    pub semantic_mass_delta: f64,
    pub runtime_state_delta: RuntimeStateDelta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedMutationValidation {
    pub execution_safe: bool,
    pub semantic_safe: bool,
    pub rollback_safe: bool,
    pub replay_invariant: bool,
    pub topology_invariant: bool,
    pub revision_consistent: bool,
    pub validation_errors: Vec<String>,
}

impl UnifiedMutationValidation {
    fn valid(&self) -> bool {
        self.execution_safe
            && self.semantic_safe
            && self.rollback_safe
            && self.replay_invariant
            && self.topology_invariant
            && self.revision_consistent
            && self.validation_errors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedRollbackChain {
    pub rollback_id: u64,
    pub execution_snapshot: ExecutionRollbackSnapshot,
    pub semantic_snapshot: SemanticRollbackSnapshot,
    pub revision_snapshot: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedMutationTransaction {
    pub transaction_id: u64,
    pub execution_mutation: Option<ExecutionApplyTransaction>,
    pub semantic_mutation: Option<SemanticRewriteTransaction>,
    pub unified_validation: UnifiedMutationValidation,
    pub unified_preview: UnifiedMutationPreview,
    pub rollback_chain: UnifiedRollbackChain,
    pub runtime_revision: u64,
    pub deterministic_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedRuntimeRevision {
    pub revision_id: u64,
    pub parent_revision: Option<u64>,
    pub execution_revision: u64,
    pub semantic_revision: u64,
    pub deterministic_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedApplyResult {
    pub applied: bool,
    pub rolled_back: bool,
    pub runtime_revision: u64,
    pub projection_synchronized: bool,
    pub checksum: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedRollbackResult {
    pub rolled_back: bool,
    pub runtime_revision: u64,
    pub projection_synchronized: bool,
    pub replay_invariant: bool,
    pub topology_invariant: bool,
    pub errors: Vec<String>,
}

pub fn unified_mutation_transaction(
    runtime: &Runtime,
    execution_mutation: Option<ExecutionApplyTransaction>,
    semantic_mutation: Option<SemanticRewriteTransaction>,
) -> UnifiedMutationTransaction {
    let rollback_chain = unified_rollback_chain(runtime);
    let unified_preview = unified_mutation_preview(
        runtime,
        execution_mutation.as_ref(),
        semantic_mutation.as_ref(),
    );
    let mut transaction = UnifiedMutationTransaction {
        transaction_id: stable_hash_u64s([
            runtime.runtime_revision,
            execution_mutation
                .as_ref()
                .map(|tx| tx.transaction_id)
                .unwrap_or(0),
            semantic_mutation
                .as_ref()
                .map(|tx| tx.transaction_id)
                .unwrap_or(0),
            rollback_chain.rollback_id,
        ]),
        execution_mutation,
        semantic_mutation,
        unified_validation: UnifiedMutationValidation {
            execution_safe: false,
            semantic_safe: false,
            rollback_safe: false,
            replay_invariant: false,
            topology_invariant: false,
            revision_consistent: false,
            validation_errors: Vec::new(),
        },
        unified_preview,
        rollback_chain,
        runtime_revision: runtime.runtime_revision,
        deterministic_checksum: 0,
    };
    transaction.unified_validation = validate_unified_mutation(&transaction);
    transaction.deterministic_checksum = deterministic_unified_checksum(&transaction);
    transaction
}

pub fn execution_apply_transaction(
    runtime: &Runtime,
    target: impl Into<String>,
    operation_count: usize,
) -> ExecutionApplyTransaction {
    let target = target.into();
    let diff = ExecutionDiff {
        deterministic_hash: stable_hash_strs([target.as_str()])
            ^ stable_hash_u64s([operation_count as u64]),
        target,
        operation_count,
    };
    let rollback_snapshot = ExecutionRollbackSnapshot {
        execution_snapshot: runtime.execution_snapshot.clone(),
        transaction_snapshot: runtime.transaction_snapshot.clone(),
    };
    let transaction_id = stable_hash_u64s([
        runtime.runtime_revision,
        diff.deterministic_hash,
        operation_count as u64,
    ]);
    let mut transaction = ExecutionApplyTransaction {
        transaction_id,
        diff,
        rollback_snapshot,
        execution_revision: runtime.runtime_revision,
        deterministic_checksum: 0,
    };
    transaction.deterministic_checksum = deterministic_execution_checksum(&transaction);
    transaction
}

pub fn unified_runtime_apply(
    runtime: &mut Runtime,
    transaction: UnifiedMutationTransaction,
) -> UnifiedApplyResult {
    let _writer = match UNIFIED_RUNTIME_WRITER.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return UnifiedApplyResult {
                applied: false,
                rolled_back: false,
                runtime_revision: runtime.runtime_revision,
                projection_synchronized: false,
                checksum: transaction.deterministic_checksum,
                errors: vec!["parallel runtime mutation rejected".to_string()],
            };
        }
        Err(TryLockError::Poisoned(_)) => {
            return UnifiedApplyResult {
                applied: false,
                rolled_back: false,
                runtime_revision: runtime.runtime_revision,
                projection_synchronized: false,
                checksum: transaction.deterministic_checksum,
                errors: vec!["runtime writer lock poisoned".to_string()],
            };
        }
    };

    let validation = validate_unified_mutation_for_runtime(runtime, &transaction);
    if !validation.valid() {
        return UnifiedApplyResult {
            applied: false,
            rolled_back: false,
            runtime_revision: runtime.runtime_revision,
            projection_synchronized: false,
            checksum: transaction.deterministic_checksum,
            errors: validation.validation_errors,
        };
    }

    if deterministic_unified_checksum(&transaction) != transaction.deterministic_checksum {
        return rollback_failed_apply(
            runtime,
            &transaction.rollback_chain,
            transaction.deterministic_checksum,
            "unified checksum mismatch",
        );
    }

    if let Some(execution) = transaction.execution_mutation.as_ref() {
        apply_execution_mutation(runtime, execution);
    }

    if let Some(semantic) = transaction.semantic_mutation.as_ref() {
        let request = RuntimeSemanticApplyRequest {
            transaction: semantic.clone(),
            validation: semantic.validation.clone(),
            apply_mode: SemanticApplyMode::Strict,
            runtime_checksum: semantic.deterministic_checksum,
        };
        let result = runtime_semantic_apply(request);
        if !result.applied {
            return rollback_failed_apply(
                runtime,
                &transaction.rollback_chain,
                transaction.deterministic_checksum,
                &result.warnings.join(", "),
            );
        }
        runtime.semantic_topology = SemanticIdentityGraph {
            identities: semantic.source_snapshot.identities.clone(),
        };
    }

    synchronize_runtime_state(runtime);
    runtime.runtime_revision = transaction.runtime_revision.saturating_add(1);
    let projection = synchronize_unified_projection(&unified_runtime_snapshot(runtime));

    UnifiedApplyResult {
        applied: true,
        rolled_back: false,
        runtime_revision: runtime.runtime_revision,
        projection_synchronized: projection.synchronized,
        checksum: transaction.deterministic_checksum,
        errors: Vec::new(),
    }
}

pub fn unified_runtime_rollback(
    runtime: &mut Runtime,
    rollback: UnifiedRollbackChain,
) -> UnifiedRollbackResult {
    runtime.execution_snapshot = rollback.execution_snapshot.execution_snapshot.clone();
    runtime.transaction_snapshot = rollback.execution_snapshot.transaction_snapshot.clone();
    runtime.semantic_topology = SemanticIdentityGraph {
        identities: rollback
            .semantic_snapshot
            .topology_snapshot
            .identities
            .clone(),
    };
    runtime.runtime_revision = rollback.revision_snapshot;
    let semantic = runtime_semantic_rollback(rollback.semantic_snapshot);
    let projection = synchronize_unified_projection(&unified_runtime_snapshot(runtime));

    UnifiedRollbackResult {
        rolled_back: semantic.restored,
        runtime_revision: runtime.runtime_revision,
        projection_synchronized: projection.synchronized,
        replay_invariant: projection.replay_invariant && semantic.replay_invariant_retained,
        topology_invariant: projection.topology_invariant,
        errors: if semantic.restored {
            Vec::new()
        } else {
            vec!["semantic rollback invariant failed".to_string()]
        },
    }
}

pub fn validate_unified_mutation(
    transaction: &UnifiedMutationTransaction,
) -> UnifiedMutationValidation {
    let mut errors = Vec::new();

    if transaction.execution_mutation.is_none() && transaction.semantic_mutation.is_none() {
        errors.push("empty unified mutation".to_string());
    }

    let execution_safe = transaction
        .execution_mutation
        .as_ref()
        .map(|execution| {
            let checksum_matches =
                deterministic_execution_checksum(execution) == execution.deterministic_checksum;
            if execution.diff.operation_count == 0 {
                errors.push("empty execution diff".to_string());
            }
            if !checksum_matches {
                errors.push("execution checksum mismatch".to_string());
            }
            execution.diff.operation_count > 0 && checksum_matches
        })
        .unwrap_or(true);

    let semantic_safe = transaction
        .semantic_mutation
        .as_ref()
        .map(|semantic| {
            let recomputed = validate_semantic_rewrite(semantic);
            let checksum = deterministic_rewrite_checksum(semantic);
            let safe = semantic.validation == recomputed
                && semantic.validation.valid
                && semantic.deterministic_checksum == checksum;
            if !safe {
                errors.push("semantic mutation validation failed".to_string());
            }
            safe
        })
        .unwrap_or(true);

    let rollback_safe = transaction.rollback_chain.rollback_id != 0
        && transaction.rollback_chain.revision_snapshot == transaction.runtime_revision;
    if !rollback_safe {
        errors.push("rollback unavailable".to_string());
    }

    let replay_invariant = transaction
        .semantic_mutation
        .as_ref()
        .map(|semantic| semantic.validation.replay_invariant)
        .unwrap_or(true);
    if !replay_invariant {
        errors.push("replay invariant violation".to_string());
    }

    let topology_invariant = transaction
        .semantic_mutation
        .as_ref()
        .map(|semantic| semantic.validation.topology_invariant)
        .unwrap_or(true);
    if !topology_invariant {
        errors.push("topology invariant violation".to_string());
    }

    let revision_consistent = transaction
        .execution_mutation
        .as_ref()
        .map(|execution| execution.execution_revision == transaction.runtime_revision)
        .unwrap_or(true);
    if !revision_consistent {
        errors.push("stale revision".to_string());
    }

    UnifiedMutationValidation {
        execution_safe,
        semantic_safe,
        rollback_safe,
        replay_invariant,
        topology_invariant,
        revision_consistent,
        validation_errors: errors,
    }
}

pub fn synchronize_runtime_state(runtime: &mut Runtime) {
    let snapshot = unified_runtime_snapshot(runtime);
    let projection = synchronize_unified_projection(&snapshot);
    runtime.transaction_snapshot.rollback_available = projection.synchronized;
    if runtime.execution_snapshot.active_transaction {
        runtime.transaction_snapshot.transaction_state = "active".to_string();
    } else if runtime.transaction_snapshot.rollback_available {
        runtime.transaction_snapshot.transaction_state = "synchronized".to_string();
    } else {
        runtime.transaction_snapshot.transaction_state = "invalid".to_string();
    }
}

pub fn unified_runtime_revision(runtime: &Runtime) -> UnifiedRuntimeRevision {
    let snapshot = unified_runtime_snapshot(runtime);
    let semantic_revision = stable_hash_u64s(
        snapshot
            .semantic_snapshot
            .topology_snapshot
            .identities
            .iter()
            .flat_map(|identity| {
                [
                    identity.identity_id,
                    identity.continuity_score.to_bits(),
                    identity.invariant_core_overlap.to_bits(),
                ]
                .into_iter()
                .chain(identity.drift_lineage.iter().copied())
            }),
    );
    let execution_revision = stable_hash_u64s([
        snapshot.execution_snapshot.branch_depth as u64,
        stable_hash_strs([snapshot.execution_snapshot.state_label.as_str()]),
        snapshot
            .execution_snapshot
            .target_label
            .as_deref()
            .map(|target| stable_hash_strs([target]))
            .unwrap_or(0),
    ]);
    let deterministic_hash = stable_hash_u64s([
        runtime.runtime_revision,
        execution_revision,
        semantic_revision,
        snapshot.projection_checksum,
    ]);

    UnifiedRuntimeRevision {
        revision_id: runtime.runtime_revision,
        parent_revision: runtime.runtime_revision.checked_sub(1),
        execution_revision,
        semantic_revision,
        deterministic_hash,
    }
}

fn validate_unified_mutation_for_runtime(
    runtime: &Runtime,
    transaction: &UnifiedMutationTransaction,
) -> UnifiedMutationValidation {
    let mut validation = validate_unified_mutation(transaction);
    if transaction.runtime_revision != runtime.runtime_revision {
        validation.revision_consistent = false;
        validation
            .validation_errors
            .push("stale revision".to_string());
    }
    if deterministic_unified_checksum(transaction) != transaction.deterministic_checksum {
        validation
            .validation_errors
            .push("unified checksum mismatch".to_string());
    }
    if let Some(semantic) = transaction.semantic_mutation.as_ref() {
        let request = RuntimeSemanticApplyRequest {
            transaction: semantic.clone(),
            validation: semantic.validation.clone(),
            apply_mode: SemanticApplyMode::Strict,
            runtime_checksum: semantic.deterministic_checksum,
        };
        let governance = validate_runtime_semantic_apply(&request);
        if !governance.allowed {
            validation.semantic_safe = false;
            validation
                .validation_errors
                .extend(governance.rejected_reason);
        }
    }
    validation.validation_errors.sort();
    validation.validation_errors.dedup();
    validation
}

fn apply_execution_mutation(runtime: &mut Runtime, execution: &ExecutionApplyTransaction) {
    runtime.execution_snapshot = ExecutionRuntimeSnapshot {
        state_label: "APPLIED".to_string(),
        target_label: Some(execution.diff.target.clone()),
        active_transaction: false,
        branch_depth: runtime.execution_snapshot.branch_depth.saturating_add(1),
    };
    runtime.transaction_snapshot = RuntimeTransactionSnapshot {
        active_transaction_id: Some(format!("tx-{}", execution.transaction_id)),
        transaction_state: "execution_applied".to_string(),
        rollback_available: true,
    };
}

fn rollback_failed_apply(
    runtime: &mut Runtime,
    rollback: &UnifiedRollbackChain,
    checksum: u64,
    reason: &str,
) -> UnifiedApplyResult {
    let result = unified_runtime_rollback(runtime, rollback.clone());
    UnifiedApplyResult {
        applied: false,
        rolled_back: result.rolled_back,
        runtime_revision: runtime.runtime_revision,
        projection_synchronized: result.projection_synchronized,
        checksum,
        errors: vec![reason.to_string()],
    }
}

pub fn unified_rollback_chain(runtime: &Runtime) -> UnifiedRollbackChain {
    let semantic_graph = SemanticIdentityGraph {
        identities: runtime.semantic_topology.identities.clone(),
    };
    let semantic_snapshot = semantic_rollback_snapshot(&semantic_graph);
    let execution_snapshot = ExecutionRollbackSnapshot {
        execution_snapshot: runtime.execution_snapshot.clone(),
        transaction_snapshot: runtime.transaction_snapshot.clone(),
    };
    let rollback_id = stable_hash_u64s([
        runtime.runtime_revision,
        semantic_snapshot.snapshot_id,
        deterministic_execution_rollback_checksum(&execution_snapshot),
    ]);

    UnifiedRollbackChain {
        rollback_id,
        execution_snapshot,
        semantic_snapshot,
        revision_snapshot: runtime.runtime_revision,
    }
}

fn unified_mutation_preview(
    runtime: &Runtime,
    execution: Option<&ExecutionApplyTransaction>,
    semantic: Option<&SemanticRewriteTransaction>,
) -> UnifiedMutationPreview {
    UnifiedMutationPreview {
        execution_diff: execution.map(|tx| tx.diff.clone()),
        topology_diff: semantic.map(|tx| tx.preview.topology_diff),
        continuity_delta: semantic
            .map(|tx| tx.preview.continuity_delta)
            .unwrap_or(0.0),
        semantic_mass_delta: semantic
            .map(|tx| tx.preview.semantic_mass_delta)
            .unwrap_or(0.0),
        runtime_state_delta: RuntimeStateDelta {
            revision_before: runtime.runtime_revision,
            revision_after: runtime.runtime_revision.saturating_add(1),
            execution_changed: execution.is_some(),
            semantic_changed: semantic.is_some(),
        },
    }
}

fn deterministic_unified_checksum(transaction: &UnifiedMutationTransaction) -> u64 {
    stable_hash_u64s([
        transaction.transaction_id,
        transaction.runtime_revision,
        transaction.rollback_chain.rollback_id,
        transaction
            .execution_mutation
            .as_ref()
            .map(|tx| tx.deterministic_checksum)
            .unwrap_or(0),
        transaction
            .semantic_mutation
            .as_ref()
            .map(|tx| tx.deterministic_checksum)
            .unwrap_or(0),
        transaction.unified_preview.continuity_delta.to_bits(),
        transaction.unified_preview.semantic_mass_delta.to_bits(),
    ])
}

fn deterministic_execution_checksum(transaction: &ExecutionApplyTransaction) -> u64 {
    stable_hash_u64s([
        transaction.transaction_id,
        transaction.execution_revision,
        transaction.diff.deterministic_hash,
        transaction.diff.operation_count as u64,
        deterministic_execution_rollback_checksum(&transaction.rollback_snapshot),
    ])
}

fn deterministic_execution_rollback_checksum(snapshot: &ExecutionRollbackSnapshot) -> u64 {
    stable_hash_u64s([
        stable_hash_strs([snapshot.execution_snapshot.state_label.as_str()]),
        snapshot
            .execution_snapshot
            .target_label
            .as_deref()
            .map(|target| stable_hash_strs([target]))
            .unwrap_or(0),
        u64::from(snapshot.execution_snapshot.active_transaction),
        snapshot.execution_snapshot.branch_depth as u64,
        snapshot
            .transaction_snapshot
            .active_transaction_id
            .as_deref()
            .map(|tx| stable_hash_strs([tx]))
            .unwrap_or(0),
        stable_hash_strs([snapshot.transaction_snapshot.transaction_state.as_str()]),
        u64::from(snapshot.transaction_snapshot.rollback_available),
    ])
}

fn stable_hash_strs<'a>(values: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_hash_u64s(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_core::{
        SemanticIdentityCandidate, SemanticIdentityGraph, semantic_rewrite_transaction,
    };
    use std::sync::Mutex;

    static TEST_RUNTIME_APPLY: Mutex<()> = Mutex::new(());

    fn semantic_transaction() -> SemanticRewriteTransaction {
        semantic_rewrite_transaction(&SemanticIdentityGraph {
            identities: vec![
                SemanticIdentityCandidate {
                    identity_id: 10,
                    continuity_score: 0.90,
                    invariant_core_overlap: 0.95,
                    drift_lineage: vec![1, 2],
                },
                SemanticIdentityCandidate {
                    identity_id: 20,
                    continuity_score: 0.82,
                    invariant_core_overlap: 0.91,
                    drift_lineage: vec![3],
                },
            ],
        })
    }

    #[test]
    fn unified_apply_is_atomic_for_execution_and_semantic() {
        let _test_lock = TEST_RUNTIME_APPLY.lock().expect("test apply lock");
        let mut runtime = Runtime::default();
        let execution = execution_apply_transaction(&runtime, "apps/cli/src/main.rs", 1);
        let semantic = semantic_transaction();
        let transaction = unified_mutation_transaction(&runtime, Some(execution), Some(semantic));

        let result = unified_runtime_apply(&mut runtime, transaction);

        assert!(result.applied);
        assert!(!result.rolled_back);
        assert_eq!(runtime.runtime_revision, 1);
        assert_eq!(runtime.execution_snapshot.state_label, "APPLIED");
        assert_eq!(runtime.semantic_topology.identities.len(), 2);
        assert!(result.projection_synchronized);
    }

    #[test]
    fn partial_mutation_is_rejected_before_apply() {
        let _test_lock = TEST_RUNTIME_APPLY.lock().expect("test apply lock");
        let mut runtime = Runtime::default();
        let mut execution = execution_apply_transaction(&runtime, "apps/cli/src/main.rs", 1);
        execution.deterministic_checksum ^= 0x55;
        let semantic = semantic_transaction();
        let transaction = unified_mutation_transaction(&runtime, Some(execution), Some(semantic));

        let result = unified_runtime_apply(&mut runtime, transaction);

        assert!(!result.applied);
        assert_eq!(runtime.runtime_revision, 0);
        assert_eq!(runtime.execution_snapshot.state_label, "IDLE");
        assert!(runtime.semantic_topology.identities.is_empty());
    }

    #[test]
    fn unified_rollback_restores_runtime_wide_state() {
        let _test_lock = TEST_RUNTIME_APPLY.lock().expect("test apply lock");
        let mut runtime = Runtime::default();
        let execution = execution_apply_transaction(&runtime, "apps/cli/src/main.rs", 1);
        let semantic = semantic_transaction();
        let rollback = unified_rollback_chain(&runtime);
        let transaction = unified_mutation_transaction(&runtime, Some(execution), Some(semantic));
        let result = unified_runtime_apply(&mut runtime, transaction);
        assert!(result.applied);

        let rollback_result = unified_runtime_rollback(&mut runtime, rollback);

        assert!(rollback_result.rolled_back);
        assert_eq!(runtime.runtime_revision, 0);
        assert_eq!(runtime.execution_snapshot.state_label, "IDLE");
        assert!(runtime.semantic_topology.identities.is_empty());
        assert!(rollback_result.projection_synchronized);
    }

    #[test]
    fn revision_lineage_is_deterministic() {
        let runtime = Runtime::default();
        let first = unified_runtime_revision(&runtime);
        let second = unified_runtime_revision(&runtime);

        assert_eq!(first, second);
        assert_eq!(first.revision_id, 0);
    }

    #[test]
    fn stale_revision_is_rejected() {
        let _test_lock = TEST_RUNTIME_APPLY.lock().expect("test apply lock");
        let mut runtime = Runtime::default();
        let execution = execution_apply_transaction(&runtime, "apps/cli/src/main.rs", 1);
        let mut transaction = unified_mutation_transaction(&runtime, Some(execution), None);
        runtime.runtime_revision = 9;
        transaction.deterministic_checksum = deterministic_unified_checksum(&transaction);

        let result = unified_runtime_apply(&mut runtime, transaction);

        assert!(!result.applied);
        assert!(result.errors.iter().any(|error| error == "stale revision"));
    }
}
