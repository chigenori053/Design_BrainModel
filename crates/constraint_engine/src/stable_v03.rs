use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraph, NodeId, NodeType};
use design_search_engine::stable_v03::ArchitectureCandidate;

pub trait Constraint: Send + Sync {
    fn check(&self, graph: &ArchitectureGraph) -> bool;
}

pub trait ConstraintEngine: Send + Sync {
    fn filter(&self, candidates: Vec<ArchitectureCandidate>) -> Vec<ArchitectureCandidate>;
}

#[derive(Clone, Default)]
pub struct CompositeConstraintEngine {
    constraints: Vec<Arc<dyn Constraint>>,
}

impl CompositeConstraintEngine {
    pub fn new(constraints: Vec<Arc<dyn Constraint>>) -> Self {
        Self { constraints }
    }
}

impl ConstraintEngine for CompositeConstraintEngine {
    fn filter(&self, candidates: Vec<ArchitectureCandidate>) -> Vec<ArchitectureCandidate> {
        candidates
            .into_iter()
            .filter(|candidate| {
                self.constraints
                    .iter()
                    .all(|constraint| constraint.check(&candidate.architecture))
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoCycleConstraint;

impl Constraint for NoCycleConstraint {
    fn check(&self, graph: &ArchitectureGraph) -> bool {
        let nodes = graph.nodes();
        let mut indegree = BTreeMap::<NodeId, usize>::new();
        let mut adjacency = BTreeMap::<NodeId, Vec<NodeId>>::new();

        for node in nodes {
            indegree.insert(node.id.clone(), 0);
        }
        for edge in graph.edges() {
            *indegree.entry(edge.target.clone()).or_insert(0) += 1;
            adjacency
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }

        let mut queue = indegree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(node_id, _)| node_id.clone())
            .collect::<VecDeque<_>>();
        let mut visited = 0usize;

        while let Some(node_id) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(&node_id) {
                for target in neighbors {
                    if let Some(count) = indegree.get_mut(target) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push_back(target.clone());
                        }
                    }
                }
            }
        }

        visited == nodes.len()
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoIsolatedNodesConstraint;

impl Constraint for NoIsolatedNodesConstraint {
    fn check(&self, graph: &ArchitectureGraph) -> bool {
        if graph.nodes().len() <= 1 {
            return true;
        }
        graph.nodes().iter().all(|node| {
            !graph.outgoing(node.id.clone()).is_empty()
                || !graph.incoming(node.id.clone()).is_empty()
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaxNodeConstraint {
    pub max_nodes: usize,
}

impl Constraint for MaxNodeConstraint {
    fn check(&self, graph: &ArchitectureGraph) -> bool {
        graph.nodes().len() <= self.max_nodes
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayerOrderConstraint;

impl Constraint for LayerOrderConstraint {
    fn check(&self, graph: &ArchitectureGraph) -> bool {
        graph.edges().iter().all(|edge| {
            let Some(source) = graph.node(&edge.source) else {
                return false;
            };
            let Some(target) = graph.node(&edge.target) else {
                return false;
            };
            layer_rank(&source.node_type) <= layer_rank(&target.node_type)
        })
    }
}

fn layer_rank(node_type: &NodeType) -> usize {
    match node_type {
        NodeType::Interface => 0,
        NodeType::Service => 1,
        NodeType::Component => 1,
        NodeType::DataStore => 2,
        NodeType::ExternalSystem => 3,
        NodeType::Custom(_) => 1,
    }
}
