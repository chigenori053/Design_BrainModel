use std::collections::HashMap;

use crate::runtime::cognitive_orchestration::{
    ConvergenceState, ExecutionMemoryRecord, SemanticMemoryState,
};
use crate::runtime::invariants::branch::{
    BranchBudget, BranchEntropyScore, OptimizedBranch, branch_entropy, optimize_branch_budget,
};
use crate::runtime::invariants::projection::ProjectionCache;
use crate::tui::rendering::ProjectionSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayDeltaOperation {
    SetProjectionHash(String),
    SetRuntimeState(String),
    AppendNarrative(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDeltaFrame {
    pub previous_hash: String,
    pub delta_operations: Vec<ReplayDeltaOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointInterval {
    pub checkpoint_every: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplayCompaction {
    pub checkpoints: Vec<String>,
    pub delta_frames: Vec<ReplayDeltaFrame>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticMemoryIndex {
    pub semantic_hash_index: HashMap<String, usize>,
    recall_cache: HashMap<String, ExecutionMemoryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMetrics {
    pub projection_count: usize,
    pub replay_frames: usize,
    pub active_branches: usize,
    pub semantic_memory_entries: usize,
}

#[derive(Debug, Clone)]
pub struct RuntimeOptimizationState {
    pub projection_cache: ProjectionCache,
    pub replay_compaction: ReplayCompaction,
    pub memory_index: SemanticMemoryIndex,
    pub metrics: RuntimeMetrics,
}

pub fn compact_replay_hashes(
    projection_hashes: &[String],
    interval: CheckpointInterval,
) -> ReplayCompaction {
    let checkpoint_every = interval.checkpoint_every.max(1);
    let mut compaction = ReplayCompaction::default();
    for (index, hash) in projection_hashes.iter().enumerate() {
        if index % checkpoint_every == 0 {
            compaction.checkpoints.push(hash.clone());
        } else {
            let previous_hash = projection_hashes
                .get(index.saturating_sub(1))
                .cloned()
                .unwrap_or_default();
            compaction.delta_frames.push(ReplayDeltaFrame {
                previous_hash,
                delta_operations: vec![ReplayDeltaOperation::SetProjectionHash(hash.clone())],
            });
        }
    }
    compaction
}

pub fn reconstruct_replay_hashes(compaction: &ReplayCompaction) -> Vec<String> {
    let next_by_previous = compaction
        .delta_frames
        .iter()
        .filter_map(|frame| {
            frame.delta_operations.iter().find_map(|operation| {
                if let ReplayDeltaOperation::SetProjectionHash(hash) = operation {
                    Some((frame.previous_hash.clone(), hash.clone()))
                } else {
                    None
                }
            })
        })
        .collect::<HashMap<_, _>>();
    let mut hashes = Vec::new();
    for checkpoint in &compaction.checkpoints {
        hashes.push(checkpoint.clone());
        let mut current = checkpoint.clone();
        while let Some(next_hash) = next_by_previous.get(&current) {
            current = next_hash.clone();
            hashes.push(current.clone());
        }
    }
    hashes.sort();
    hashes.dedup();
    hashes
}

pub fn build_semantic_memory_index(memory: &SemanticMemoryState) -> SemanticMemoryIndex {
    let mut records = memory.execution_records.clone();
    records.sort_by(|a, b| a.semantic_hash.cmp(&b.semantic_hash));
    let mut semantic_hash_index = HashMap::new();
    let mut recall_cache = HashMap::new();
    for (index, record) in records.into_iter().enumerate() {
        semantic_hash_index.insert(record.semantic_hash.clone(), index);
        recall_cache.insert(record.semantic_hash.clone(), record);
    }
    SemanticMemoryIndex {
        semantic_hash_index,
        recall_cache,
    }
}

pub fn recall_indexed_memory(
    index: &SemanticMemoryIndex,
    semantic_hash: &str,
) -> Option<ExecutionMemoryRecord> {
    index.recall_cache.get(semantic_hash).cloned()
}

pub fn compact_semantic_memory(memory: &mut SemanticMemoryState) {
    memory
        .execution_records
        .sort_by(|a, b| a.semantic_hash.cmp(&b.semantic_hash));
    memory.execution_records.dedup_by(|a, b| {
        a.semantic_hash == b.semantic_hash && a.convergence_result != ConvergenceState::Collapsed
    });
}

pub fn optimize_runtime_state(
    snapshots: Vec<ProjectionSnapshot>,
    replay_hashes: &[String],
    memory: &mut SemanticMemoryState,
    branches: &[crate::runtime::cognitive_orchestration::BranchEvaluation],
    branch_budget: &BranchBudget,
) -> (
    RuntimeOptimizationState,
    Vec<OptimizedBranch>,
    BranchEntropyScore,
) {
    let mut projection_cache = ProjectionCache::default();
    for snapshot in snapshots {
        projection_cache.get_or_insert(snapshot);
    }
    compact_semantic_memory(memory);
    let memory_index = build_semantic_memory_index(memory);
    let replay_compaction = compact_replay_hashes(
        replay_hashes,
        CheckpointInterval {
            checkpoint_every: 4,
        },
    );
    let optimized_branches = optimize_branch_budget(branches, branch_budget);
    let branch_entropy = branch_entropy(branches);
    let metrics = RuntimeMetrics {
        projection_count: projection_cache.len(),
        replay_frames: replay_compaction.delta_frames.len() + replay_compaction.checkpoints.len(),
        active_branches: optimized_branches
            .iter()
            .filter(|branch| !branch.pruned)
            .count(),
        semantic_memory_entries: memory.execution_records.len(),
    };
    (
        RuntimeOptimizationState {
            projection_cache,
            replay_compaction,
            memory_index,
            metrics,
        },
        optimized_branches,
        branch_entropy,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::runtime::autonomous_control::RiskLevel;
    use crate::runtime::cognitive_orchestration::{
        AttentionState, BranchEvaluation, ExecutionMemoryRecord,
    };
    use crate::runtime::execution_governance::ExecutionResult;
    use crate::runtime::invariants::attention::{
        AttentionDecay, AttentionInvariantSuite, decay_attention,
    };
    use crate::runtime::invariants::branch::BranchInvariantSuite;
    use crate::runtime::invariants::projection::{ProjectionCache, ProjectionInvariantSuite};
    use crate::tui::rendering::{
        DiagnosticProjection, NarrativeProjection, ProjectionHash, WorkspaceProjection,
        projection_semantic_hash,
    };

    fn projection(status: &str) -> ProjectionSnapshot {
        let mut snapshot = ProjectionSnapshot {
            workspace: WorkspaceProjection {
                target: Some("apps/cli/src/core.rs".to_string()),
                operation: "runtime".to_string(),
                status: status.to_string(),
            },
            diagnostics: DiagnosticProjection::default(),
            narrative: NarrativeProjection {
                lines: vec![status.to_string()],
            },
            runtime_state: status.to_string(),
            projection_hash: ProjectionHash::default(),
        };
        snapshot.projection_hash.semantic_hash = projection_semantic_hash(&snapshot);
        snapshot
    }

    #[test]
    fn projection_compression_reuses_same_arc_snapshot() {
        let snapshot = projection("stable");
        let mut cache = ProjectionCache::default();
        let first = cache.get_or_insert(snapshot.clone());
        let second = cache.get_or_insert(snapshot);

        ProjectionInvariantSuite::assert_projection_reused(&first, &second);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn replay_compaction_preserves_hash_set() {
        let hashes = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let compaction = compact_replay_hashes(
            &hashes,
            CheckpointInterval {
                checkpoint_every: 2,
            },
        );
        let mut reconstructed = reconstruct_replay_hashes(&compaction);
        let mut expected = hashes;
        reconstructed.sort();
        expected.sort();

        assert_eq!(reconstructed, expected);
        assert!(compaction.delta_frames.len() < expected.len());
    }

    #[test]
    fn branch_pruning_prevents_pruned_branch_mutation() {
        let branches = vec![
            BranchEvaluation {
                branch_id: "a".to_string(),
                semantic_score: 0.9,
                projected_risk: RiskLevel::Low,
                convergence_score: 0.9,
            },
            BranchEvaluation {
                branch_id: "b".to_string(),
                semantic_score: 0.1,
                projected_risk: RiskLevel::Critical,
                convergence_score: 0.1,
            },
        ];
        let budget = BranchBudget {
            max_active_branches: 1,
            max_speculative_branches: 1,
        };
        let optimized = optimize_branch_budget(&branches, &budget);

        BranchInvariantSuite::assert_budget_respected(&optimized, &budget);
        assert!(optimized.iter().any(|branch| branch.pruned));
    }

    #[test]
    fn attention_decay_decreases_inactive_saliency() {
        let before = AttentionState {
            focused_goals: vec!["g2".to_string(), "g1".to_string(), "g1".to_string()],
            suppressed_contexts: vec!["old".to_string()],
            attention_score: 0.8,
        };
        let after = decay_attention(&before, AttentionDecay::bounded(0.5));

        AttentionInvariantSuite::assert_bounded_attention(&after);
        AttentionInvariantSuite::assert_decay_decreases(&before, &after);
        assert_eq!(after.attention_score, 0.4);
        assert_eq!(
            after.focused_goals,
            vec!["g1".to_string(), "g2".to_string()]
        );
    }

    #[test]
    fn semantic_memory_index_recalls_deterministically() {
        let mut memory = SemanticMemoryState {
            execution_records: vec![
                ExecutionMemoryRecord {
                    semantic_hash: "b".to_string(),
                    execution_result: ExecutionResult {
                        status: "ok".to_string(),
                        summary: "second".to_string(),
                    },
                    convergence_result: ConvergenceState::Stable,
                },
                ExecutionMemoryRecord {
                    semantic_hash: "a".to_string(),
                    execution_result: ExecutionResult {
                        status: "ok".to_string(),
                        summary: "first".to_string(),
                    },
                    convergence_result: ConvergenceState::Stable,
                },
            ],
        };

        compact_semantic_memory(&mut memory);
        let index = build_semantic_memory_index(&memory);
        let recalled = recall_indexed_memory(&index, "a").expect("memory exists");

        assert_eq!(index.semantic_hash_index.get("a"), Some(&0));
        assert_eq!(recalled.execution_result.summary, "first");
    }
}
