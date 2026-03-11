use std::collections::{BTreeMap, BTreeSet};

use architecture_reasoner::ArchitectureGraph;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureMetrics {
    pub modularity: f64,
    pub coupling: f64,
    pub cohesion: f64,
    pub layering_score: f64,
    pub dependency_entropy: f64,
}

#[derive(Clone, Debug, Default)]
pub struct MetricsCalculator;

impl MetricsCalculator {
    pub fn compute(&self, graph: &ArchitectureGraph) -> ArchitectureMetrics {
        let dependency_edges = graph.dependency_edges().collect::<Vec<_>>();
        let node_count = graph.nodes.len().max(1) as f64;
        let layer_map = graph.layer_map();
        let coupling = (dependency_edges.len() as f64 / node_count).clamp(0.0, 1.0);
        let same_layer_edges = dependency_edges
            .iter()
            .filter(|edge| layer_map.get(&edge.from) == layer_map.get(&edge.to))
            .count();
        let modularity = if dependency_edges.is_empty() {
            1.0
        } else {
            same_layer_edges as f64 / dependency_edges.len() as f64
        };
        let layering_score = if dependency_edges.is_empty() {
            1.0
        } else {
            dependency_edges
                .iter()
                .filter(|edge| {
                    matches!(
                        (layer_map.get(&edge.from), layer_map.get(&edge.to)),
                        (Some(from), Some(to)) if from.order() <= to.order()
                    )
                })
                .count() as f64
                / dependency_edges.len() as f64
        };
        let cohesion = layer_groups(graph)
            .values()
            .map(|members| (members.len() as f64 / graph.nodes.len().max(1) as f64).powi(2))
            .sum::<f64>()
            .clamp(0.0, 1.0);

        ArchitectureMetrics {
            modularity,
            coupling,
            cohesion,
            layering_score,
            dependency_entropy: dependency_entropy(graph),
        }
    }
}

fn layer_groups(graph: &ArchitectureGraph) -> BTreeMap<String, BTreeSet<u64>> {
    let mut groups = BTreeMap::<String, BTreeSet<u64>>::new();
    for node in &graph.nodes {
        groups
            .entry(node.layer.as_str().to_string())
            .or_default()
            .insert(node.id);
    }
    groups
}

fn dependency_entropy(graph: &ArchitectureGraph) -> f64 {
    let edges = graph.dependency_edges().collect::<Vec<_>>();
    if edges.is_empty() {
        return 0.0;
    }
    let layer_map = graph.layer_map();
    let mut counts = BTreeMap::<(usize, usize), usize>::new();
    for edge in edges {
        if let (Some(from), Some(to)) = (layer_map.get(&edge.from), layer_map.get(&edge.to)) {
            *counts.entry((from.order(), to.order())).or_default() += 1;
        }
    }
    let total = counts.values().sum::<usize>() as f64;
    let raw = counts.values().fold(0.0, |sum, count| {
        let p = *count as f64 / total;
        if p == 0.0 {
            sum
        } else {
            sum - p * p.log2()
        }
    });
    let max = (counts.len().max(1) as f64).log2().max(1.0);
    (raw / max).clamp(0.0, 1.0)
}
