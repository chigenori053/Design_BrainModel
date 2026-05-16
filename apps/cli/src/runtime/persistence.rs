use crate::runtime::unified_apply::{
    UnifiedRollbackChain, UnifiedRuntimeRevision, unified_rollback_chain, unified_runtime_revision,
};
use crate::runtime::unified_projection::{
    Runtime, RuntimeTransactionSnapshot, semantic_runtime_snapshot, synchronize_unified_projection,
    unified_runtime_snapshot,
};
use memory_space_core::{SemanticAttractor, SemanticIdentityGraph, SemanticIdentitySnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct PersistentRuntimeMemory {
    pub memory_id: u64,
    pub runtime_revision_chain: Vec<UnifiedRuntimeRevision>,
    pub topology_history: Vec<SemanticIdentitySnapshot>,
    pub attractor_history: Vec<Vec<SemanticAttractor>>,
    pub rollback_history: Vec<UnifiedRollbackChain>,
    pub persistence_checksum: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CognitiveSessionState {
    pub session_id: u64,
    pub current_revision: u64,
    pub active_topology: SemanticIdentitySnapshot,
    pub active_attractors: Vec<SemanticAttractor>,
    pub continuity_score: f64,
    pub stabilized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentRevisionLineage {
    pub lineage_id: u64,
    pub revisions: Vec<UnifiedRuntimeRevision>,
    pub deterministic_hash_chain: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeMemoryCheckpoint {
    pub checkpoint_id: u64,
    pub revision_snapshot: UnifiedRuntimeRevision,
    pub topology_snapshot: SemanticIdentitySnapshot,
    pub attractor_snapshot: Vec<SemanticAttractor>,
    pub rollback_snapshot: UnifiedRollbackChain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvolutionEventType {
    TopologyEvolution,
    AttractorEvolution,
    StabilizationEvolution,
    DriftRecoveryEvolution,
    CheckpointCreated,
    RollbackPersisted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMemoryEvolutionEvent {
    pub event_id: u64,
    pub revision_id: u64,
    pub evolution_type: EvolutionEventType,
    pub deterministic_timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RestoreResult {
    pub restored: bool,
    pub runtime: Runtime,
    pub replay_invariant: bool,
    pub topology_invariant: bool,
    pub revision_invariant: bool,
    pub errors: Vec<String>,
}

pub fn persistent_runtime_memory(runtime: &Runtime) -> PersistentRuntimeMemory {
    let checkpoint = runtime_memory_checkpoint(runtime);
    let mut memory = PersistentRuntimeMemory {
        memory_id: stable_hash_u64s([
            checkpoint.revision_snapshot.revision_id,
            checkpoint.checkpoint_id,
            checkpoint.rollback_snapshot.rollback_id,
        ]),
        runtime_revision_chain: vec![checkpoint.revision_snapshot.clone()],
        topology_history: vec![checkpoint.topology_snapshot.clone()],
        attractor_history: vec![checkpoint.attractor_snapshot.clone()],
        rollback_history: vec![checkpoint.rollback_snapshot.clone()],
        persistence_checksum: 0,
    };
    memory.persistence_checksum = deterministic_persistence_checksum(&memory);
    memory
}

pub fn cognitive_session_state(runtime: &Runtime) -> CognitiveSessionState {
    let semantic = semantic_runtime_snapshot(&runtime.semantic_topology);
    let continuity_score = average_continuity(&semantic.topology_snapshot);
    CognitiveSessionState {
        session_id: stable_hash_u64s([
            runtime.runtime_revision,
            semantic.topology_snapshot.identities.len() as u64,
            semantic.attractor_snapshot.len() as u64,
        ]),
        current_revision: runtime.runtime_revision,
        active_topology: semantic.topology_snapshot,
        active_attractors: semantic.attractor_snapshot,
        continuity_score,
        stabilized: continuity_score >= 0.50,
    }
}

pub fn runtime_memory_checkpoint(runtime: &Runtime) -> RuntimeMemoryCheckpoint {
    let semantic = semantic_runtime_snapshot(&runtime.semantic_topology);
    let revision_snapshot = unified_runtime_revision(runtime);
    let rollback_snapshot = unified_rollback_chain(runtime);
    let checkpoint_id = stable_hash_u64s([
        revision_snapshot.revision_id,
        revision_snapshot.deterministic_hash,
        rollback_snapshot.rollback_id,
    ]);
    RuntimeMemoryCheckpoint {
        checkpoint_id,
        revision_snapshot,
        topology_snapshot: semantic.topology_snapshot,
        attractor_snapshot: semantic.attractor_snapshot,
        rollback_snapshot,
    }
}

pub fn restore_runtime_checkpoint(checkpoint: RuntimeMemoryCheckpoint) -> RestoreResult {
    let mut runtime = Runtime {
        runtime_revision: checkpoint.revision_snapshot.revision_id,
        execution_snapshot: checkpoint
            .rollback_snapshot
            .execution_snapshot
            .execution_snapshot
            .clone(),
        semantic_topology: SemanticIdentityGraph {
            identities: checkpoint.topology_snapshot.identities.clone(),
        },
        transaction_snapshot: checkpoint
            .rollback_snapshot
            .execution_snapshot
            .transaction_snapshot
            .clone(),
    };
    if runtime
        .transaction_snapshot
        .transaction_state
        .trim()
        .is_empty()
    {
        runtime.transaction_snapshot = RuntimeTransactionSnapshot {
            active_transaction_id: None,
            transaction_state: "restored".to_string(),
            rollback_available: checkpoint
                .rollback_snapshot
                .execution_snapshot
                .transaction_snapshot
                .rollback_available,
        };
    }
    let projection = synchronize_unified_projection(&unified_runtime_snapshot(&runtime));
    let restored_revision = unified_runtime_revision(&runtime);
    let revision_invariant =
        restored_revision.deterministic_hash == checkpoint.revision_snapshot.deterministic_hash;
    let restored =
        projection.replay_invariant && projection.topology_invariant && revision_invariant;
    RestoreResult {
        restored,
        runtime,
        replay_invariant: projection.replay_invariant,
        topology_invariant: projection.topology_invariant,
        revision_invariant,
        errors: if restored {
            Vec::new()
        } else {
            vec!["checkpoint restore invariant failed".to_string()]
        },
    }
}

pub fn persistent_revision_lineage(runtime: &Runtime) -> PersistentRevisionLineage {
    let revision = unified_runtime_revision(runtime);
    let deterministic_hash_chain = vec![revision.deterministic_hash];
    PersistentRevisionLineage {
        lineage_id: stable_hash_u64s([
            revision.revision_id,
            revision.parent_revision.unwrap_or(0),
            revision.deterministic_hash,
        ]),
        revisions: vec![revision],
        deterministic_hash_chain,
    }
}

pub fn validate_persistent_runtime(memory: &PersistentRuntimeMemory) -> bool {
    if memory.runtime_revision_chain.is_empty()
        || memory.topology_history.is_empty()
        || memory.attractor_history.is_empty()
        || memory.rollback_history.is_empty()
    {
        return false;
    }
    if deterministic_persistence_checksum(memory) != memory.persistence_checksum {
        return false;
    }
    memory
        .runtime_revision_chain
        .windows(2)
        .all(|pair| pair[0].revision_id < pair[1].revision_id)
}

pub fn append_runtime_memory_checkpoint(
    memory: &mut PersistentRuntimeMemory,
    checkpoint: RuntimeMemoryCheckpoint,
) -> bool {
    if memory
        .runtime_revision_chain
        .last()
        .is_some_and(|last| checkpoint.revision_snapshot.revision_id <= last.revision_id)
    {
        return false;
    }
    memory
        .runtime_revision_chain
        .push(checkpoint.revision_snapshot);
    memory.topology_history.push(checkpoint.topology_snapshot);
    memory.attractor_history.push(checkpoint.attractor_snapshot);
    memory.rollback_history.push(checkpoint.rollback_snapshot);
    memory.persistence_checksum = deterministic_persistence_checksum(memory);
    true
}

pub fn runtime_memory_evolution_events(
    memory: &PersistentRuntimeMemory,
) -> Vec<RuntimeMemoryEvolutionEvent> {
    let mut events = Vec::new();
    for revision in &memory.runtime_revision_chain {
        events.push(EvolutionEventType::CheckpointCreated);
        events.push(EvolutionEventType::RollbackPersisted);
        events.push(EvolutionEventType::TopologyEvolution);
        events.push(EvolutionEventType::AttractorEvolution);
        if revision.semantic_revision != 0 {
            events.push(EvolutionEventType::StabilizationEvolution);
            events.push(EvolutionEventType::DriftRecoveryEvolution);
        }
    }
    events.sort();
    events.dedup();
    events
        .into_iter()
        .enumerate()
        .map(|(index, evolution_type)| RuntimeMemoryEvolutionEvent {
            event_id: stable_hash_u64s([memory.memory_id, index as u64, evolution_type as u64]),
            revision_id: memory
                .runtime_revision_chain
                .last()
                .map(|revision| revision.revision_id)
                .unwrap_or(0),
            evolution_type,
            deterministic_timestamp: (index as u64).saturating_add(
                memory
                    .runtime_revision_chain
                    .last()
                    .map(|revision| revision.revision_id.saturating_mul(1_000))
                    .unwrap_or(0),
            ),
        })
        .collect()
}

pub fn render_persistent_runtime_memory(memory: &PersistentRuntimeMemory) -> String {
    format!(
        "memory_id: {}\nrevision_count: {}\ntopology_history: {}\nattractor_history: {}\nrollback_history: {}\npersistence_checksum: {}\nvalid: {}",
        memory.memory_id,
        memory.runtime_revision_chain.len(),
        memory.topology_history.len(),
        memory.attractor_history.len(),
        memory.rollback_history.len(),
        memory.persistence_checksum,
        validate_persistent_runtime(memory),
    )
}

pub fn render_cognitive_session_state(state: &CognitiveSessionState) -> String {
    format!(
        "session_id: {}\ncurrent_revision: {}\nactive_identities: {}\nactive_attractors: {}\ncontinuity_score: {:.6}\nstabilized: {}",
        state.session_id,
        state.current_revision,
        state.active_topology.identities.len(),
        state.active_attractors.len(),
        state.continuity_score,
        state.stabilized,
    )
}

pub fn render_checkpoint(checkpoint: &RuntimeMemoryCheckpoint) -> String {
    format!(
        "checkpoint_id: {}\nrevision: {}\ntopology_identities: {}\nattractors: {}\nrollback_id: {}",
        checkpoint.checkpoint_id,
        checkpoint.revision_snapshot.revision_id,
        checkpoint.topology_snapshot.identities.len(),
        checkpoint.attractor_snapshot.len(),
        checkpoint.rollback_snapshot.rollback_id,
    )
}

pub fn render_lineage(lineage: &PersistentRevisionLineage) -> String {
    format!(
        "lineage_id: {}\nrevisions: {}\nhash_chain: {:?}",
        lineage.lineage_id,
        lineage.revisions.len(),
        lineage.deterministic_hash_chain,
    )
}

pub fn render_evolution_events(events: &[RuntimeMemoryEvolutionEvent]) -> String {
    let mut lines = vec![format!("evolution_events: {}", events.len())];
    for event in events {
        lines.push(format!(
            "event: id={} revision={} type={:?} timestamp={}",
            event.event_id, event.revision_id, event.evolution_type, event.deterministic_timestamp
        ));
    }
    lines.join("\n")
}

fn deterministic_persistence_checksum(memory: &PersistentRuntimeMemory) -> u64 {
    let mut values = vec![memory.memory_id];
    for revision in &memory.runtime_revision_chain {
        values.extend([
            revision.revision_id,
            revision.parent_revision.unwrap_or(0),
            revision.execution_revision,
            revision.semantic_revision,
            revision.deterministic_hash,
        ]);
    }
    for topology in &memory.topology_history {
        values.push(topology.timestamp);
        for identity in &topology.identities {
            values.extend([
                identity.identity_id,
                identity.continuity_score.to_bits(),
                identity.invariant_core_overlap.to_bits(),
            ]);
            values.extend(identity.drift_lineage.iter().copied());
        }
    }
    for attractors in &memory.attractor_history {
        for attractor in attractors {
            values.extend([
                attractor.attractor_id,
                attractor.invariant_density.to_bits(),
                attractor.stability_score.to_bits(),
                attractor.attractor_strength.to_bits(),
                attractor.semantic_mass.to_bits(),
            ]);
        }
    }
    for rollback in &memory.rollback_history {
        values.extend([rollback.rollback_id, rollback.revision_snapshot]);
    }
    stable_hash_u64s(values)
}

fn average_continuity(snapshot: &SemanticIdentitySnapshot) -> f64 {
    if snapshot.identities.is_empty() {
        return 1.0;
    }
    snapshot
        .identities
        .iter()
        .map(|identity| identity.continuity_score.clamp(0.0, 1.0))
        .sum::<f64>()
        / snapshot.identities.len() as f64
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
    use crate::runtime::unified_apply::{
        execution_apply_transaction, unified_mutation_transaction, unified_runtime_apply,
    };
    use memory_space_core::{
        SemanticIdentityCandidate, SemanticIdentityGraph, semantic_rewrite_transaction,
    };
    use std::sync::Mutex;

    static TEST_PERSISTENCE_APPLY: Mutex<()> = Mutex::new(());

    fn evolved_runtime() -> Runtime {
        let mut runtime = Runtime::default();
        let execution = execution_apply_transaction(&runtime, "apps/cli/src/main.rs", 1);
        let semantic = semantic_rewrite_transaction(&SemanticIdentityGraph {
            identities: vec![
                SemanticIdentityCandidate {
                    identity_id: 1,
                    continuity_score: 0.90,
                    invariant_core_overlap: 0.95,
                    drift_lineage: vec![10],
                },
                SemanticIdentityCandidate {
                    identity_id: 2,
                    continuity_score: 0.82,
                    invariant_core_overlap: 0.91,
                    drift_lineage: vec![20],
                },
            ],
        });
        let tx = unified_mutation_transaction(&runtime, Some(execution), Some(semantic));
        let result = unified_runtime_apply(&mut runtime, tx);
        assert!(result.applied);
        runtime
    }

    #[test]
    fn runtime_persistence_is_deterministic() {
        let _test_lock = TEST_PERSISTENCE_APPLY
            .lock()
            .expect("test persistence lock");
        let runtime = evolved_runtime();
        let first = persistent_runtime_memory(&runtime);
        let second = persistent_runtime_memory(&runtime);

        assert_eq!(first, second);
        assert!(validate_persistent_runtime(&first));
    }

    #[test]
    fn checkpoint_restore_is_replay_invariant() {
        let _test_lock = TEST_PERSISTENCE_APPLY
            .lock()
            .expect("test persistence lock");
        let runtime = evolved_runtime();
        let checkpoint = runtime_memory_checkpoint(&runtime);

        let restored = restore_runtime_checkpoint(checkpoint);

        assert!(restored.restored);
        assert!(restored.replay_invariant);
        assert!(restored.topology_invariant);
        assert!(restored.revision_invariant);
    }

    #[test]
    fn revision_lineage_is_append_only() {
        let runtime = Runtime::default();
        let mut memory = persistent_runtime_memory(&runtime);
        let mut evolved = runtime.clone();
        evolved.runtime_revision = 1;
        let checkpoint = runtime_memory_checkpoint(&evolved);

        assert!(append_runtime_memory_checkpoint(
            &mut memory,
            checkpoint.clone()
        ));
        assert!(!append_runtime_memory_checkpoint(&mut memory, checkpoint));
        assert_eq!(memory.runtime_revision_chain.len(), 2);
    }

    #[test]
    fn evolution_events_are_deterministic() {
        let _test_lock = TEST_PERSISTENCE_APPLY
            .lock()
            .expect("test persistence lock");
        let runtime = evolved_runtime();
        let memory = persistent_runtime_memory(&runtime);

        let first = runtime_memory_evolution_events(&memory);
        let second = runtime_memory_evolution_events(&memory);

        assert_eq!(first, second);
        assert!(
            first
                .windows(2)
                .all(|pair| pair[0].evolution_type <= pair[1].evolution_type)
        );
    }

    #[test]
    fn cognitive_session_retains_continuity() {
        let _test_lock = TEST_PERSISTENCE_APPLY
            .lock()
            .expect("test persistence lock");
        let runtime = evolved_runtime();
        let state = cognitive_session_state(&runtime);

        assert_eq!(state.current_revision, runtime.runtime_revision);
        assert!(state.continuity_score >= 0.50);
        assert!(state.stabilized);
    }
}
