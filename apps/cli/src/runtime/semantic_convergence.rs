use std::collections::{BTreeMap, HashMap};

use crate::runtime::cognitive_orchestration::{
    AttentionState, BranchEvaluation, CognitiveState, SemanticMemoryState,
};
use crate::runtime::convergence_optimization::{
    CheckpointInterval, ReplayCompaction, compact_replay_hashes, reconstruct_replay_hashes,
};
use crate::runtime::invariants::branch::branch_entropy;
use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub runtime_id: String,
    pub persistent_semantic_signature: String,
    pub convergence_lineage: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionGroup {
    pub equivalence_hash: String,
    pub projection_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCompression {
    pub compressed_projection_groups: Vec<ProjectionGroup>,
    pub semantic_equivalence_map: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PredictiveScheduler {
    pub projected_runtime_cost: RuntimeCost,
    pub projected_memory_growth: usize,
    pub projected_branch_growth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeCost {
    pub projection_cost: f64,
    pub replay_cost: f64,
    pub memory_cost: f64,
    pub orchestration_cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchedulingDecision {
    ExecuteNow,
    Delay,
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveAttentionField {
    pub contextual_saliency: f64,
    pub convergence_focus: f64,
    pub runtime_noise_level: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeEvaluation {
    pub convergence_quality: f64,
    pub runtime_stability: f64,
    pub semantic_density: f64,
    pub orchestration_efficiency: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConvergenceState {
    pub convergence_score: f64,
    pub semantic_noise: f64,
    pub replay_stability: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayReconstruction {
    pub reconstructed_projection_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEquivalence {
    pub equivalence_hash: String,
    pub equivalent_projection_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpandedRuntimeMetrics {
    pub semantic_density: f64,
    pub convergence_velocity: f64,
    pub replay_stability: f64,
    pub branch_entropy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticConvergenceHalt {
    SemanticDivergenceOverflow,
    ReplayInstability,
    ConvergenceCollapse,
    RuntimeEntropyExplosion,
    IdentityDrift,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticConvergenceRuntime {
    pub identity: RuntimeIdentity,
    pub compression: SemanticCompression,
    pub scheduler: PredictiveScheduler,
    pub attention_field: AdaptiveAttentionField,
    pub evaluation: RuntimeEvaluation,
    pub convergence_state: ConvergenceState,
    pub replay_reconstruction: ReplayReconstruction,
    pub metrics: ExpandedRuntimeMetrics,
    pub halt: Option<SemanticConvergenceHalt>,
}

pub fn runtime_identity(cognitive_state: &CognitiveState, lineage: Vec<String>) -> RuntimeIdentity {
    let mut convergence_lineage = lineage;
    convergence_lineage.sort();
    convergence_lineage.dedup();
    let persistent_semantic_signature =
        identity_signature(&cognitive_state.cognitive_id, &convergence_lineage);
    RuntimeIdentity {
        runtime_id: format!("runtime-{persistent_semantic_signature}"),
        persistent_semantic_signature,
        convergence_lineage,
    }
}

pub fn compress_semantics(snapshots: &[ProjectionSnapshot]) -> SemanticCompression {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for snapshot in snapshots {
        let projection_hash = projection_semantic_hash(snapshot);
        let equivalence_hash = projection_equivalence_hash(snapshot);
        groups
            .entry(equivalence_hash)
            .or_default()
            .push(projection_hash);
    }

    let mut compressed_projection_groups = Vec::new();
    let mut semantic_equivalence_map = HashMap::new();
    for (equivalence_hash, mut projection_hashes) in groups {
        projection_hashes.sort();
        projection_hashes.dedup();
        if let Some(canonical) = projection_hashes.first().cloned() {
            for projection_hash in &projection_hashes {
                semantic_equivalence_map.insert(projection_hash.clone(), canonical.clone());
            }
        }
        compressed_projection_groups.push(ProjectionGroup {
            equivalence_hash,
            projection_hashes,
        });
    }

    SemanticCompression {
        compressed_projection_groups,
        semantic_equivalence_map,
    }
}

pub fn semantic_equivalence(snapshots: &[ProjectionSnapshot]) -> Vec<SemanticEquivalence> {
    compress_semantics(snapshots)
        .compressed_projection_groups
        .into_iter()
        .map(|group| SemanticEquivalence {
            equivalence_hash: group.equivalence_hash,
            equivalent_projection_ids: group.projection_hashes,
        })
        .collect()
}

pub fn reconstruct_replay(compaction: &ReplayCompaction) -> ReplayReconstruction {
    let hashes = reconstruct_replay_hashes(compaction);
    ReplayReconstruction {
        reconstructed_projection_hash: replay_reconstruction_hash(&hashes),
    }
}

pub fn predictive_scheduler(
    metrics: ExpandedRuntimeMetrics,
    projection_count: usize,
    replay_frame_count: usize,
    memory_entries: usize,
    branch_count: usize,
) -> PredictiveScheduler {
    PredictiveScheduler {
        projected_runtime_cost: RuntimeCost {
            projection_cost: normalized_cost(projection_count, 64),
            replay_cost: (normalized_cost(replay_frame_count, 128)
                * (1.0 + (1.0 - metrics.replay_stability)))
                .clamp(0.0, 1.0),
            memory_cost: normalized_cost(memory_entries, 256),
            orchestration_cost: ((metrics.semantic_density + metrics.branch_entropy) / 2.0)
                .clamp(0.0, 1.0),
        },
        projected_memory_growth: memory_entries
            .saturating_add((metrics.semantic_density * 10.0) as usize),
        projected_branch_growth: branch_count
            .saturating_add((metrics.branch_entropy * 4.0) as usize),
    }
}

pub fn scheduling_decision(scheduler: &PredictiveScheduler) -> SchedulingDecision {
    let total = total_cost(scheduler.projected_runtime_cost);
    if total >= 0.90 {
        SchedulingDecision::Halt
    } else if total >= 0.65
        || scheduler.projected_memory_growth > 512
        || scheduler.projected_branch_growth > 16
    {
        SchedulingDecision::Delay
    } else {
        SchedulingDecision::ExecuteNow
    }
}

pub fn adaptive_attention_field(
    attention: &AttentionState,
    goal_relevance: f64,
    branch_convergence: f64,
    runtime_stability: f64,
) -> AdaptiveAttentionField {
    let focused_ratio = (attention.focused_goals.len() as f64 / 3.0).clamp(0.0, 1.0);
    let noise = (1.0 - runtime_stability)
        .max(attention.attention_score - focused_ratio)
        .clamp(0.0, 1.0);
    AdaptiveAttentionField {
        contextual_saliency: ((goal_relevance * 0.7) + (focused_ratio * 0.3)).clamp(0.0, 1.0),
        convergence_focus: ((branch_convergence * 0.6) + (runtime_stability * 0.4)).clamp(0.0, 1.0),
        runtime_noise_level: noise,
    }
}

pub fn evaluate_runtime(
    metrics: ExpandedRuntimeMetrics,
    compression: &SemanticCompression,
    scheduler: &PredictiveScheduler,
) -> RuntimeEvaluation {
    let compression_ratio = if compression.semantic_equivalence_map.is_empty() {
        1.0
    } else {
        compression.compressed_projection_groups.len() as f64
            / compression.semantic_equivalence_map.len() as f64
    };
    let cost = total_cost(scheduler.projected_runtime_cost);
    RuntimeEvaluation {
        convergence_quality: (metrics.convergence_velocity * metrics.replay_stability)
            .clamp(0.0, 1.0),
        runtime_stability: (1.0 - metrics.branch_entropy)
            .min(metrics.replay_stability)
            .clamp(0.0, 1.0),
        semantic_density: (metrics.semantic_density * compression_ratio).clamp(0.0, 1.0),
        orchestration_efficiency: (1.0 - cost).clamp(0.0, 1.0),
    }
}

pub fn convergence_state(
    evaluation: RuntimeEvaluation,
    metrics: ExpandedRuntimeMetrics,
) -> ConvergenceState {
    let semantic_noise = (metrics.semantic_density + metrics.branch_entropy) / 2.0;
    ConvergenceState {
        convergence_score: ((evaluation.convergence_quality + evaluation.runtime_stability) / 2.0)
            .clamp(0.0, 1.0),
        semantic_noise: semantic_noise.clamp(0.0, 1.0),
        replay_stability: metrics.replay_stability.clamp(0.0, 1.0),
    }
}

pub fn expanded_metrics(
    snapshots: &[ProjectionSnapshot],
    replay_compaction: &ReplayCompaction,
    memory: &SemanticMemoryState,
    branches: &[BranchEvaluation],
    previous_convergence_score: f64,
    current_convergence_score: f64,
) -> ExpandedRuntimeMetrics {
    let unique_projection_hashes = snapshots
        .iter()
        .map(projection_semantic_hash)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let semantic_density = if snapshots.is_empty() {
        0.0
    } else {
        1.0 - (unique_projection_hashes as f64 / snapshots.len() as f64)
    }
    .clamp(0.0, 1.0);
    let replay_hashes = reconstruct_replay_hashes(replay_compaction);
    let replay_stability = if replay_hashes.is_empty() {
        1.0
    } else {
        replay_hashes.len() as f64
            / (replay_compaction.checkpoints.len() + replay_compaction.delta_frames.len()).max(1)
                as f64
    }
    .clamp(0.0, 1.0);
    let branch_entropy = branch_entropy(branches).entropy;
    let memory_pressure = normalized_cost(memory.execution_records.len(), 256);
    ExpandedRuntimeMetrics {
        semantic_density: (semantic_density + memory_pressure) / 2.0,
        convergence_velocity: (current_convergence_score - previous_convergence_score)
            .max(0.0)
            .clamp(0.0, 1.0),
        replay_stability,
        branch_entropy,
    }
}

pub fn run_semantic_convergence(
    cognitive_state: &CognitiveState,
    snapshots: &[ProjectionSnapshot],
    replay_hashes: &[String],
    memory: &SemanticMemoryState,
    branches: &[BranchEvaluation],
    previous_convergence_score: f64,
    current_convergence_score: f64,
) -> SemanticConvergenceRuntime {
    let replay_compaction = compact_replay_hashes(
        replay_hashes,
        CheckpointInterval {
            checkpoint_every: 4,
        },
    );
    let metrics = expanded_metrics(
        snapshots,
        &replay_compaction,
        memory,
        branches,
        previous_convergence_score,
        current_convergence_score,
    );
    let identity = runtime_identity(cognitive_state, replay_hashes.to_vec());
    let compression = compress_semantics(snapshots);
    let scheduler = predictive_scheduler(
        metrics,
        snapshots.len(),
        replay_compaction.delta_frames.len() + replay_compaction.checkpoints.len(),
        memory.execution_records.len(),
        branches.len(),
    );
    let attention_field = adaptive_attention_field(
        &cognitive_state.attention_state,
        1.0 - metrics.semantic_density,
        1.0 - metrics.branch_entropy,
        metrics.replay_stability,
    );
    let evaluation = evaluate_runtime(metrics, &compression, &scheduler);
    let convergence_state = convergence_state(evaluation, metrics);
    let replay_reconstruction = reconstruct_replay(&replay_compaction);
    let halt = detect_halt(&identity, &convergence_state, metrics);

    SemanticConvergenceRuntime {
        identity,
        compression,
        scheduler,
        attention_field,
        evaluation,
        convergence_state,
        replay_reconstruction,
        metrics,
        halt,
    }
}

fn detect_halt(
    identity: &RuntimeIdentity,
    convergence_state: &ConvergenceState,
    metrics: ExpandedRuntimeMetrics,
) -> Option<SemanticConvergenceHalt> {
    if identity.persistent_semantic_signature.is_empty() {
        return Some(SemanticConvergenceHalt::IdentityDrift);
    }
    if convergence_state.semantic_noise > 0.95 {
        return Some(SemanticConvergenceHalt::SemanticDivergenceOverflow);
    }
    if convergence_state.replay_stability < 0.5 {
        return Some(SemanticConvergenceHalt::ReplayInstability);
    }
    if convergence_state.convergence_score < 0.05 {
        return Some(SemanticConvergenceHalt::ConvergenceCollapse);
    }
    if metrics.branch_entropy > 0.95 && metrics.semantic_density > 0.80 {
        return Some(SemanticConvergenceHalt::RuntimeEntropyExplosion);
    }
    None
}

fn projection_equivalence_hash(snapshot: &ProjectionSnapshot) -> String {
    stable_hash_hex([
        snapshot.workspace.target.as_deref().unwrap_or(""),
        &snapshot.workspace.operation,
        &snapshot.workspace.status,
        &snapshot.runtime_state,
    ])
}

fn identity_signature(cognitive_id: &str, lineage: &[String]) -> String {
    let mut parts = vec![cognitive_id.to_string()];
    parts.extend(lineage.iter().cloned());
    stable_hash_hex(parts.iter().map(String::as_str))
}

fn replay_reconstruction_hash(hashes: &[String]) -> String {
    stable_hash_hex(hashes.iter().map(String::as_str))
}

fn normalized_cost(value: usize, budget: usize) -> f64 {
    if budget == 0 {
        1.0
    } else {
        (value as f64 / budget as f64).clamp(0.0, 1.0)
    }
}

fn total_cost(cost: RuntimeCost) -> f64 {
    ((cost.projection_cost + cost.replay_cost + cost.memory_cost + cost.orchestration_cost) / 4.0)
        .clamp(0.0, 1.0)
}

fn stable_hash_hex<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        hash ^= value.len() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::autonomous_control::RiskLevel;
    use crate::runtime::cognitive_orchestration::{
        BranchEvaluation, GoalPriority, GoalStatus, cognitive_state, goal_state,
    };
    use crate::tui::rendering::{
        DiagnosticProjection, NarrativeProjection, ProjectionHash, WorkspaceProjection,
    };

    fn projection(status: &str, narrative: &str) -> ProjectionSnapshot {
        let mut snapshot = ProjectionSnapshot {
            workspace: WorkspaceProjection {
                target: Some("apps/cli/src/core.rs".to_string()),
                operation: "runtime".to_string(),
                status: status.to_string(),
            },
            diagnostics: DiagnosticProjection::default(),
            narrative: NarrativeProjection {
                lines: vec![narrative.to_string()],
            },
            runtime_state: status.to_string(),
            projection_hash: ProjectionHash::default(),
        };
        snapshot.projection_hash.semantic_hash = projection_semantic_hash(&snapshot);
        snapshot
    }

    fn state() -> CognitiveState {
        cognitive_state(
            vec![goal_state(
                "stabilize semantic runtime",
                GoalPriority::Critical,
                GoalStatus::Executing,
                vec![],
            )],
            projection("stable", "base"),
        )
    }

    #[test]
    fn compression_preserves_semantic_replay_equivalence() {
        let snapshots = vec![
            projection("stable", "line-a"),
            projection("stable", "line-b"),
            projection("changed", "line-c"),
        ];
        let compression = compress_semantics(&snapshots);

        assert_eq!(compression.compressed_projection_groups.len(), 2);
        assert_eq!(compression.semantic_equivalence_map.len(), 3);
    }

    #[test]
    fn same_runtime_lineage_produces_same_identity_signature() {
        let state = state();
        let first = runtime_identity(&state, vec!["b".to_string(), "a".to_string()]);
        let second = runtime_identity(&state, vec!["a".to_string(), "b".to_string()]);

        assert_eq!(first, second);
        assert!(!first.persistent_semantic_signature.is_empty());
    }

    #[test]
    fn high_relevance_context_receives_higher_saliency() {
        let attention = AttentionState {
            focused_goals: vec!["goal-a".to_string()],
            suppressed_contexts: Vec::new(),
            attention_score: 0.2,
        };
        let low = adaptive_attention_field(&attention, 0.1, 0.5, 0.8);
        let high = adaptive_attention_field(&attention, 0.9, 0.5, 0.8);

        assert!(high.contextual_saliency > low.contextual_saliency);
    }

    #[test]
    fn high_projected_cost_delays_scheduling() {
        let scheduler = predictive_scheduler(
            ExpandedRuntimeMetrics {
                semantic_density: 0.9,
                convergence_velocity: 0.1,
                replay_stability: 0.2,
                branch_entropy: 0.9,
            },
            60,
            120,
            220,
            12,
        );

        assert_ne!(
            scheduling_decision(&scheduler),
            SchedulingDecision::ExecuteNow
        );
    }

    #[test]
    fn same_semantic_state_reconstructs_same_replay_hash() {
        let hashes = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let first = reconstruct_replay(&compact_replay_hashes(
            &hashes,
            CheckpointInterval {
                checkpoint_every: 2,
            },
        ));
        let second = reconstruct_replay(&compact_replay_hashes(
            &hashes,
            CheckpointInterval {
                checkpoint_every: 2,
            },
        ));

        assert_eq!(first, second);
    }

    #[test]
    fn semantic_convergence_runtime_is_deterministic() {
        let state = state();
        let snapshots = vec![projection("stable", "a"), projection("stable", "b")];
        let replay_hashes = vec!["a".to_string(), "b".to_string()];
        let branches = vec![BranchEvaluation {
            branch_id: "main".to_string(),
            semantic_score: 0.9,
            projected_risk: RiskLevel::Low,
            convergence_score: 0.9,
        }];
        let memory = SemanticMemoryState::default();

        let first = run_semantic_convergence(
            &state,
            &snapshots,
            &replay_hashes,
            &memory,
            &branches,
            0.2,
            0.8,
        );
        let second = run_semantic_convergence(
            &state,
            &snapshots,
            &replay_hashes,
            &memory,
            &branches,
            0.2,
            0.8,
        );

        assert_eq!(first, second);
        assert!(first.halt.is_none());
    }
}
