use crate::{DesignGraph, DesignNode, DesignNodeId, DesignNodeKind};

pub struct DesignQuery<'a> {
    graph: &'a DesignGraph,
}

impl<'a> DesignQuery<'a> {
    pub fn new(graph: &'a DesignGraph) -> Self {
        Self { graph }
    }

    pub fn find_by_kind(&self, kind: DesignNodeKind) -> Vec<&'a DesignNode> {
        self.graph.find_by_kind(kind)
    }

    pub fn dependencies(&self, node_id: DesignNodeId) -> Vec<DesignNodeId> {
        self.graph.dependencies(node_id)
    }
}
