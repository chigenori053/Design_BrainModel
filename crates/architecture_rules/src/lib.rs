use std::collections::{BTreeMap, BTreeSet};

use architecture_reasoner::ArchitectureGraph;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchitectureRule {
    NoDependencyCycle,
    LayerViolation,
    BoundedContextViolation,
    ForbiddenDependency,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleViolation {
    pub rule: ArchitectureRule,
    pub message: String,
    pub nodes: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct RuleValidator;

impl RuleValidator {
    pub fn validate(&self, graph: &ArchitectureGraph) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        let layer_map = graph.layer_map();
        let dependencies = graph.dependency_edges().collect::<Vec<_>>();

        for edge in &dependencies {
            if let (Some(from), Some(to)) = (layer_map.get(&edge.from), layer_map.get(&edge.to)) {
                if from.order() > to.order() {
                    violations.push(RuleViolation {
                        rule: ArchitectureRule::LayerViolation,
                        message: format!("layer violation {} -> {}", edge.from, edge.to),
                        nodes: vec![edge.from, edge.to],
                    });
                }
                if from.order() == 3 && to.order() == 0 {
                    violations.push(RuleViolation {
                        rule: ArchitectureRule::ForbiddenDependency,
                        message: format!("forbidden dependency {} -> {}", edge.from, edge.to),
                        nodes: vec![edge.from, edge.to],
                    });
                }
            }
        }

        for cycle in find_two_node_cycles(&dependencies) {
            violations.push(RuleViolation {
                rule: ArchitectureRule::NoDependencyCycle,
                message: "dependency cycle".to_string(),
                nodes: vec![cycle.0, cycle.1],
            });
        }

        if graph
            .nodes
            .iter()
            .any(|node| node.name.to_ascii_lowercase().contains("gateway"))
            && graph
                .nodes
                .iter()
                .any(|node| node.name.to_ascii_lowercase().contains("repository"))
        {
            let bounded = graph.dependency_edges().any(|edge| {
                match (layer_map.get(&edge.from), layer_map.get(&edge.to)) {
                    (Some(from), Some(to)) => from.order() + 2 < to.order(),
                    _ => false,
                }
            });
            if bounded {
                violations.push(RuleViolation {
                    rule: ArchitectureRule::BoundedContextViolation,
                    message: "cross-bounded-context dependency".to_string(),
                    nodes: Vec::new(),
                });
            }
        }

        violations
    }
}

fn find_two_node_cycles(
    dependencies: &[&architecture_reasoner::ArchitectureEdge],
) -> BTreeSet<(u64, u64)> {
    let mut cycles = BTreeSet::new();
    let adjacency = dependencies
        .iter()
        .map(|edge| ((edge.from, edge.to), true))
        .collect::<BTreeMap<_, _>>();
    for edge in dependencies {
        if adjacency.contains_key(&(edge.to, edge.from)) {
            cycles.insert((edge.from.min(edge.to), edge.from.max(edge.to)));
        }
    }
    cycles
}
