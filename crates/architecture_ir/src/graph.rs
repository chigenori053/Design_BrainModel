use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use petgraph::dot::{Config, Dot};
use petgraph::graph::{DiGraph, NodeIndex};

use crate::{ArchitectureIR, DependencyType, NodeId};

#[derive(Clone, Debug)]
pub struct ArchitectureGraph {
    pub graph: DiGraph<NodeId, DependencyType>,
    pub node_index: HashMap<NodeId, NodeIndex>,
}

impl ArchitectureGraph {
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

pub fn architecture_hash(ir: &ArchitectureIR) -> u64 {
    let mut hasher = DefaultHasher::new();
    ir.hash(&mut hasher);
    hasher.finish()
}

pub fn export_dot(ir: &ArchitectureIR) -> String {
    let graph = ir.to_graph();
    format!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]))
}
