use std::collections::{BTreeMap, BTreeSet};

use architecture_reasoner::{ArchitectureEdgeKind, ArchitectureGraph};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryReport {
    pub layer_violations: usize,
    pub circular_dependencies: usize,
    pub coupling_distance: f64,
    pub architecture_symmetry: f64,
    pub modularity: f64,
    pub structural_coherence: f64,
    pub dependency_entropy: f64,
    pub graph_curvature: f64,
}

#[derive(Clone, Debug, Default)]
pub struct GeometryEngine;

impl GeometryEngine {
    pub fn evaluate(&self, graph: &ArchitectureGraph) -> GeometryReport {
        let layers = graph.layer_map();
        let dependency_edges = graph
            .edges
            .iter()
            .filter(|edge| matches!(edge.kind, ArchitectureEdgeKind::Dependency))
            .collect::<Vec<_>>();

        let layer_violations = dependency_edges
            .iter()
            .filter(|edge| {
                let from = layers.get(&edge.from).copied();
                let to = layers.get(&edge.to).copied();
                matches!((from, to), (Some(a), Some(b)) if a.order() > b.order())
            })
            .count();
        let circular_dependencies = count_two_cycles(&dependency_edges);
        let coupling_distance = if dependency_edges.is_empty() {
            0.0
        } else {
            dependency_edges
                .iter()
                .filter_map(|edge| {
                    Some(
                        (layers.get(&edge.from)?.order() as isize
                            - layers.get(&edge.to)?.order() as isize)
                            .unsigned_abs() as f64,
                    )
                })
                .sum::<f64>()
                / dependency_edges.len() as f64
        };
        let architecture_symmetry = symmetry_score(graph);
        let modularity = modularity_score(graph, &layers);
        let dependency_entropy = entropy(&dependency_edges, &layers);
        let structural_coherence = (1.0
            - (layer_violations as f64 * 0.2 + circular_dependencies as f64 * 0.25))
            .clamp(0.0, 1.0);
        let graph_curvature =
            (modularity + architecture_symmetry + (1.0 - dependency_entropy)) / 3.0;

        GeometryReport {
            layer_violations,
            circular_dependencies,
            coupling_distance,
            architecture_symmetry,
            modularity,
            structural_coherence,
            dependency_entropy,
            graph_curvature,
        }
    }
}

fn count_two_cycles(edges: &[&architecture_reasoner::ArchitectureEdge]) -> usize {
    let mut pairs = BTreeSet::new();
    for edge in edges {
        if edges
            .iter()
            .any(|other| other.from == edge.to && other.to == edge.from)
        {
            let key = if edge.from < edge.to {
                (edge.from, edge.to)
            } else {
                (edge.to, edge.from)
            };
            pairs.insert(key);
        }
    }
    pairs.len()
}

fn symmetry_score(graph: &ArchitectureGraph) -> f64 {
    if graph.nodes.is_empty() {
        return 1.0;
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for node in &graph.nodes {
        *counts.entry(node.layer.as_str().to_string()).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0) as f64;
    let min = counts.values().copied().min().unwrap_or(0) as f64;
    if max == 0.0 {
        1.0
    } else {
        (min / max).clamp(0.0, 1.0)
    }
}

fn modularity_score(
    graph: &ArchitectureGraph,
    layers: &BTreeMap<u64, design_domain::Layer>,
) -> f64 {
    let dependencies = graph.dependency_edges().collect::<Vec<_>>();
    if dependencies.is_empty() {
        return 1.0;
    }
    let intra = dependencies
        .iter()
        .filter(|edge| layers.get(&edge.from) == layers.get(&edge.to))
        .count();
    (intra as f64 / dependencies.len() as f64).clamp(0.0, 1.0)
}

fn entropy(
    edges: &[&architecture_reasoner::ArchitectureEdge],
    layers: &BTreeMap<u64, design_domain::Layer>,
) -> f64 {
    if edges.is_empty() {
        return 0.0;
    }
    let mut counts = BTreeMap::<(usize, usize), usize>::new();
    for edge in edges {
        if let (Some(from), Some(to)) = (layers.get(&edge.from), layers.get(&edge.to)) {
            *counts.entry((from.order(), to.order())).or_default() += 1;
        }
    }
    let total = edges.len() as f64;
    let raw = counts.values().fold(0.0, |sum, count| {
        let p = *count as f64 / total;
        if p == 0.0 { sum } else { sum - p * p.log2() }
    });
    let max = (counts.len().max(1) as f64).log2().max(1.0);
    (raw / max).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use architecture_reasoner::ReverseArchitectureReasoner;
    use code_ir::CodeIr;
    use design_domain::DesignUnit;

    use super::*;

    #[test]
    fn detects_layer_violation_and_cycle() {
        let mut repository = DesignUnit::new(1, "UserRepository");
        repository.dependencies.push(design_domain::DesignUnitId(2));
        let mut service = DesignUnit::new(2, "UserService");
        service.dependencies.push(design_domain::DesignUnitId(1));

        let graph = ReverseArchitectureReasoner
            .infer_from_code_ir(&CodeIr::from_design_units(&[repository, service]));
        let report = GeometryEngine.evaluate(&graph);

        assert_eq!(report.layer_violations, 1);
        assert_eq!(report.circular_dependencies, 1);
    }
}
