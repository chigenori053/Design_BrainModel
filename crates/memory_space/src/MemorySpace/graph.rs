use std::collections::{BTreeMap, BTreeSet};

use super::types::{MemoryEdge, MemoryId, MemoryMetadata, MemoryNode, MemoryType, RelationType};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryGraph {
    nodes: BTreeMap<MemoryId, MemoryNode>,
    edges: Vec<MemoryEdge>,
    next_id: MemoryId,
}

impl MemoryGraph {
    pub fn add_node(
        &mut self,
        node_type: MemoryType,
        embedding: Vec<f32>,
        metadata: MemoryMetadata,
    ) -> MemoryId {
        self.next_id += 1;
        let node_id = self.next_id;
        self.nodes.insert(
            node_id,
            MemoryNode {
                node_id,
                node_type,
                embedding,
                metadata,
            },
        );
        node_id
    }

    pub fn add_edge(&mut self, from: MemoryId, to: MemoryId, relation: RelationType, weight: f32) {
        let edge = MemoryEdge {
            from,
            to,
            relation,
            weight: weight.clamp(0.0, 1.0),
        };
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
            self.edges.sort_by(|lhs, rhs| {
                lhs.from
                    .cmp(&rhs.from)
                    .then_with(|| lhs.to.cmp(&rhs.to))
                    .then_with(|| lhs.relation.cmp(&rhs.relation))
            });
        }
    }

    pub fn get(&self, node_id: MemoryId) -> Option<&MemoryNode> {
        self.nodes.get(&node_id)
    }

    pub fn nodes(&self) -> Vec<&MemoryNode> {
        self.nodes.values().collect()
    }

    pub fn edges(&self) -> &[MemoryEdge] {
        &self.edges
    }

    pub fn nearest_search(&self, query: &[f32], top_k: usize) -> Vec<(MemoryId, f32)> {
        let mut scored = self
            .nodes
            .values()
            .map(|node| (node.node_id, cosine_similarity(query, &node.embedding)))
            .collect::<Vec<_>>();
        scored.sort_by(|(lid, ls), (rid, rs)| rs.total_cmp(ls).then_with(|| lid.cmp(rid)));
        scored.truncate(top_k.max(1));
        scored
    }

    pub fn activation_propagation(
        &self,
        initial: &[(MemoryId, f32)],
        top_k: usize,
        max_hops: usize,
    ) -> Vec<(MemoryId, f32)> {
        let mut activation = BTreeMap::<MemoryId, f32>::new();
        for (node_id, score) in initial {
            activation.insert(*node_id, (*score).clamp(0.0, 1.0));
        }

        let mut frontier = initial
            .iter()
            .map(|(node_id, _)| *node_id)
            .collect::<BTreeSet<_>>();
        for _ in 0..max_hops {
            let mut next_frontier = BTreeSet::new();
            let mut pending = Vec::new();
            for node_id in &frontier {
                let current = activation.get(node_id).copied().unwrap_or(0.0);
                for edge in self.edges.iter().filter(|edge| edge.from == *node_id) {
                    let propagated = (current * edge.weight).clamp(0.0, 1.0);
                    let existing = activation.get(&edge.to).copied().unwrap_or(0.0);
                    if propagated > existing {
                        pending.push((edge.to, propagated));
                        next_frontier.insert(edge.to);
                    }
                }
            }
            if pending.is_empty() {
                break;
            }
            for (node_id, score) in pending {
                activation.insert(node_id, score);
            }
            frontier = next_frontier;
        }

        let mut ranked = activation.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|(lid, ls), (rid, rs)| rs.total_cmp(ls).then_with(|| lid.cmp(rid)));
        ranked.truncate(top_k.max(1));
        ranked
    }
}

fn cosine_similarity(lhs: &[f32], rhs: &[f32]) -> f32 {
    let len = lhs.len().min(rhs.len());
    if len == 0 {
        return 0.0;
    }
    let lhs = &lhs[..len];
    let rhs = &rhs[..len];
    let dot = lhs.iter().zip(rhs.iter()).map(|(a, b)| a * b).sum::<f32>();
    let lhs_norm = lhs.iter().map(|value| value * value).sum::<f32>().sqrt();
    let rhs_norm = rhs.iter().map(|value| value * value).sum::<f32>().sqrt();
    if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.0
    } else {
        (dot / (lhs_norm * rhs_norm)).clamp(0.0, 1.0)
    }
}
