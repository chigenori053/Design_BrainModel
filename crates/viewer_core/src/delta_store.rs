use std::collections::BTreeMap;

use crate::model::{RiskOverlay, SnapshotDelta, SnapshotGraph, ViewEdge};

pub fn apply_snapshot_delta(
    mut graph: SnapshotGraph,
    delta: &SnapshotDelta,
) -> (SnapshotGraph, Vec<RiskOverlay>) {
    let mut nodes = graph
        .nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    for update in &delta.node_updates {
        match &update.after {
            Some(node) => {
                nodes.insert(update.id.clone(), node.clone());
            }
            None => {
                nodes.remove(&update.id);
            }
        }
    }

    let mut edges = graph
        .edges
        .into_iter()
        .map(|edge| (edge_key(&edge), edge))
        .collect::<BTreeMap<_, _>>();
    for update in &delta.edge_updates {
        let key = format!("{}|{}|{}", update.from, update.to, update.kind);
        match &update.after {
            Some(edge) => {
                edges.insert(key, edge.clone());
            }
            None => {
                edges.remove(&key);
            }
        }
    }

    let overlays = delta
        .overlay_updates
        .iter()
        .filter_map(|update| update.after.clone())
        .collect::<Vec<_>>();

    graph.nodes = nodes.into_values().collect();
    graph.edges = edges.into_values().collect();
    (graph, overlays)
}

fn edge_key(edge: &ViewEdge) -> String {
    format!("{}|{}|{}", edge.from, edge.to, edge.kind)
}
