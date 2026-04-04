use crate::delta_store::apply_snapshot_delta;
use crate::model::{
    EdgeDeltaDelta, NodeDelta, SnapshotDelta, SnapshotGraph, StructureSnapshot, Timeline3D,
    ViewEdge,
};

pub fn resolve_tick(timeline: &Timeline3D, time: f64) -> usize {
    if timeline.snapshots.is_empty() {
        return 0;
    }
    if timeline.autoplay {
        (time as usize) % timeline.snapshots.len()
    } else {
        timeline.current_tick.min(timeline.snapshots.len() - 1)
    }
}

pub fn rebuild_scene_from_deltas(snapshots: &[StructureSnapshot]) -> Vec<SnapshotGraph> {
    let mut rebuilt = Vec::new();
    let mut current: Option<SnapshotGraph> = None;
    for snapshot in snapshots {
        if let Some(base) = snapshot.base.clone() {
            current = Some(base);
        }
        let Some(graph) = current.clone() else {
            continue;
        };
        let (next, _) = apply_snapshot_delta(graph, &snapshot.delta);
        current = Some(next.clone());
        rebuilt.push(next);
    }
    rebuilt
}

pub fn compact_delta_chain(
    snapshots: &[StructureSnapshot],
    max_chain: usize,
) -> Vec<StructureSnapshot> {
    if snapshots.len() <= max_chain {
        return snapshots.to_vec();
    }

    let rebuilt = rebuild_scene_from_deltas(snapshots);
    if rebuilt.is_empty() {
        return snapshots.to_vec();
    }

    let mut compacted = Vec::new();
    let mut start = 0usize;
    while start < snapshots.len() {
        let end = (start + max_chain).min(snapshots.len());
        let base_graph = if start == 0 {
            snapshots[start]
                .base
                .clone()
                .unwrap_or_else(|| rebuilt[start].clone())
        } else {
            rebuilt[start - 1].clone()
        };
        let end_graph = rebuilt[end - 1].clone();
        let template = &snapshots[end - 1];
        compacted.push(StructureSnapshot {
            base: Some(base_graph.clone()),
            delta: diff_snapshot_graphs(&base_graph, &end_graph, &template.delta.summary),
            timestamp: template.timestamp.clone(),
            action: template.action.clone(),
            confidence: template.confidence,
        });
        start = end;
    }
    compacted
}

fn diff_snapshot_graphs(
    before: &SnapshotGraph,
    after: &SnapshotGraph,
    summary: &[String],
) -> SnapshotDelta {
    let before_nodes = before
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_nodes = after
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let ids = before_nodes
        .keys()
        .chain(after_nodes.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let node_updates = ids
        .into_iter()
        .filter_map(|id| {
            let left = before_nodes.get(&id).cloned();
            let right = after_nodes.get(&id).cloned();
            (left != right).then_some(NodeDelta {
                id,
                before: left,
                after: right,
            })
        })
        .collect::<Vec<_>>();

    let before_edges = before
        .edges
        .iter()
        .map(|edge| (edge_key(edge), edge.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_edges = after
        .edges
        .iter()
        .map(|edge| (edge_key(edge), edge.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let edge_keys = before_edges
        .keys()
        .chain(after_edges.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let edge_updates = edge_keys
        .into_iter()
        .filter_map(|key| {
            let left = before_edges.get(&key).cloned();
            let right = after_edges.get(&key).cloned();
            if left == right {
                return None;
            }
            let (from, to, kind) = split_edge_key(&key);
            Some(EdgeDeltaDelta {
                from,
                to,
                kind,
                before: left,
                after: right,
            })
        })
        .collect::<Vec<_>>();

    SnapshotDelta {
        summary: summary.to_vec(),
        node_updates,
        edge_updates,
        overlay_updates: Vec::new(),
    }
}

fn edge_key(edge: &ViewEdge) -> String {
    format!("{}|{}|{}", edge.from, edge.to, edge.kind)
}

fn split_edge_key(key: &str) -> (String, String, String) {
    let mut parts = key.split('|');
    (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeDeltaDelta, SnapshotDelta, StructureSnapshot, ViewEdge, ViewNode};

    use super::*;

    #[test]
    fn rebuild_scene_from_deltas_test() {
        let base = SnapshotGraph {
            nodes: vec![ViewNode {
                id: "a".to_string(),
                label: "a".to_string(),
                layer: 0,
                role: "Core".to_string(),
                x: 0.0,
                y: 10.0,
                z: 0.0,
            }],
            edges: vec![],
        };
        let snapshots = vec![
            StructureSnapshot {
                base: Some(base.clone()),
                delta: SnapshotDelta {
                    node_updates: vec![crate::model::NodeDelta {
                        id: "a".to_string(),
                        before: Some(base.nodes[0].clone()),
                        after: Some(ViewNode {
                            x: 10.0,
                            ..base.nodes[0].clone()
                        }),
                    }],
                    ..Default::default()
                },
                timestamp: "1".to_string(),
                action: "preview".to_string(),
                confidence: 1.0,
            },
            StructureSnapshot {
                base: None,
                delta: SnapshotDelta {
                    edge_updates: vec![EdgeDeltaDelta {
                        from: "a".to_string(),
                        to: "b".to_string(),
                        kind: "depends_on".to_string(),
                        before: None,
                        after: Some(ViewEdge {
                            from: "a".to_string(),
                            to: "b".to_string(),
                            kind: "depends_on".to_string(),
                            cycle: false,
                        }),
                    }],
                    ..Default::default()
                },
                timestamp: "2".to_string(),
                action: "apply".to_string(),
                confidence: 1.0,
            },
        ];
        let rebuilt = super::rebuild_scene_from_deltas(&snapshots);
        assert_eq!(rebuilt.len(), 2);
        assert_eq!(rebuilt[0].nodes[0].x, 10.0);
        assert_eq!(rebuilt[1].edges.len(), 1);
    }

    #[test]
    fn compacts_delta_chain() {
        let base = SnapshotGraph {
            nodes: vec![ViewNode {
                id: "a".to_string(),
                label: "a".to_string(),
                layer: 0,
                role: "Core".to_string(),
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }],
            edges: vec![],
        };
        let mut snapshots = Vec::new();
        let mut last = base.clone();
        for index in 0..120 {
            let next = SnapshotGraph {
                nodes: vec![ViewNode {
                    x: index as f32 + 1.0,
                    ..last.nodes[0].clone()
                }],
                edges: if index % 3 == 0 {
                    vec![ViewEdge {
                        from: "a".to_string(),
                        to: format!("b{index}"),
                        kind: "depends_on".to_string(),
                        cycle: false,
                    }]
                } else {
                    last.edges.clone()
                },
            };
            snapshots.push(StructureSnapshot {
                base: if index == 0 { Some(last.clone()) } else { None },
                delta: diff_snapshot_graphs(&last, &next, &[format!("delta {index}")]),
                timestamp: index.to_string(),
                action: "apply".to_string(),
                confidence: 1.0,
            });
            last = next;
        }
        let compacted = compact_delta_chain(&snapshots, 100);
        assert!(compacted.len() < snapshots.len());
        let rebuilt = super::rebuild_scene_from_deltas(&compacted);
        let original = super::rebuild_scene_from_deltas(&snapshots);
        assert_eq!(rebuilt.last(), original.last());
    }
}
