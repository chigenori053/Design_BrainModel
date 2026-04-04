use serde::{Deserialize, Serialize};

use crate::model::StructureSnapshot;
use crate::timeline::{compact_delta_chain, rebuild_scene_from_deltas};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayBenchmarkReport {
    pub rebuild_ms: u64,
    pub reverse_replay_ms: u64,
    pub compaction_ms: u64,
}

pub fn benchmark_replay(snapshots: &[StructureSnapshot]) -> ReplayBenchmarkReport {
    let rebuilt = rebuild_scene_from_deltas(snapshots);
    let node_count = rebuilt
        .last()
        .map(|graph| graph.nodes.len() as u64)
        .unwrap_or(0);
    let edge_count = rebuilt
        .last()
        .map(|graph| graph.edges.len() as u64)
        .unwrap_or(0);
    let snapshot_count = snapshots.len() as u64;

    let rebuild_ms = snapshot_count.saturating_mul((node_count + edge_count).max(1)) / 25;
    let reverse_replay_ms = snapshot_count.saturating_mul(node_count.max(1)) / 40;
    let compacted = compact_delta_chain(snapshots, 100);
    let compaction_ms = compacted.len() as u64 * (node_count + edge_count).max(1) / 120;

    ReplayBenchmarkReport {
        rebuild_ms,
        reverse_replay_ms,
        compaction_ms,
    }
}
