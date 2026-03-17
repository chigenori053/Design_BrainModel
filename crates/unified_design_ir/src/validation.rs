use std::collections::{BTreeMap, VecDeque};

use crate::{DesignGraph, DesignRelation};

pub trait DesignValidator: Send + Sync {
    fn validate(&self, graph: &DesignGraph) -> bool;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultDesignValidator;

impl DesignValidator for DefaultDesignValidator {
    fn validate(&self, graph: &DesignGraph) -> bool {
        has_defined_edges(graph)
            && !has_isolated_nodes(graph)
            && !has_cycle(graph)
            && has_valid_relations(graph)
    }
}

fn has_defined_edges(graph: &DesignGraph) -> bool {
    graph
        .edges()
        .iter()
        .all(|edge| graph.node(&edge.source).is_some() && graph.node(&edge.target).is_some())
}

fn has_isolated_nodes(graph: &DesignGraph) -> bool {
    graph.nodes().iter().any(|node| {
        graph.dependencies(node.id.clone()).is_empty()
            && !graph.edges().iter().any(|edge| edge.target == node.id)
    })
}

fn has_valid_relations(graph: &DesignGraph) -> bool {
    graph.edges().iter().all(|edge| {
        matches!(
            edge.relation,
            DesignRelation::DependsOn
                | DesignRelation::Calls
                | DesignRelation::Owns
                | DesignRelation::Implements
        )
    })
}

fn has_cycle(graph: &DesignGraph) -> bool {
    let mut indegree = BTreeMap::new();
    let mut adjacency = BTreeMap::new();
    for node in graph.nodes() {
        indegree.insert(node.id.clone(), 0usize);
    }
    for edge in graph.edges() {
        *indegree.entry(edge.target.clone()).or_insert(0) += 1;
        adjacency
            .entry(edge.source.clone())
            .or_insert_with(Vec::new)
            .push(edge.target.clone());
    }
    let mut queue = indegree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect::<VecDeque<_>>();
    let mut visited = 0usize;
    while let Some(node_id) = queue.pop_front() {
        visited += 1;
        if let Some(targets) = adjacency.get(&node_id) {
            for target in targets {
                if let Some(entry) = indegree.get_mut(target) {
                    *entry -= 1;
                    if *entry == 0 {
                        queue.push_back(target.clone());
                    }
                }
            }
        }
    }
    visited != graph.nodes().len()
}
