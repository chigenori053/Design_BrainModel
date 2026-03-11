use architecture_reasoner::{ArchitectureEdgeKind, ArchitectureGraph};

pub type ComponentId = u64;
pub type NodeId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionNode {
    Component(ComponentId),
    ExternalService(String),
    Database(String),
    Queue(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionEdgeType {
    SyncCall,
    AsyncMessage,
    DataAccess,
    EventEmit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub edge_type: ExecutionEdgeType,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionGraph {
    pub nodes: Vec<ExecutionNode>,
    pub edges: Vec<ExecutionEdge>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionGraphBuilder;

impl ExecutionGraphBuilder {
    pub fn build(&self, graph: &ArchitectureGraph) -> ExecutionGraph {
        let nodes = graph
            .nodes
            .iter()
            .map(|node| classify_node(&node.name, node.id))
            .collect::<Vec<_>>();
        let edges = graph
            .edges
            .iter()
            .map(|edge| ExecutionEdge {
                source: edge.from,
                target: edge.to,
                edge_type: classify_edge(edge.kind, graph, edge.to),
            })
            .collect::<Vec<_>>();
        ExecutionGraph { nodes, edges }
    }
}

fn classify_node(name: &str, id: u64) -> ExecutionNode {
    let lower = name.to_ascii_lowercase();
    if lower.contains("database") || lower.contains("repository") || lower.contains("store") {
        ExecutionNode::Database(name.to_string())
    } else if lower.contains("queue") || lower.contains("broker") || lower.contains("stream") {
        ExecutionNode::Queue(name.to_string())
    } else if lower.contains("client") || lower.contains("external") {
        ExecutionNode::ExternalService(name.to_string())
    } else {
        ExecutionNode::Component(id)
    }
}

fn classify_edge(
    kind: ArchitectureEdgeKind,
    graph: &ArchitectureGraph,
    target: u64,
) -> ExecutionEdgeType {
    let target_name = graph
        .nodes
        .iter()
        .find(|node| node.id == target)
        .map(|node| node.name.to_ascii_lowercase())
        .unwrap_or_default();
    match kind {
        ArchitectureEdgeKind::Dependency => {
            if target_name.contains("queue") || target_name.contains("broker") {
                ExecutionEdgeType::AsyncMessage
            } else if target_name.contains("database") || target_name.contains("repository") {
                ExecutionEdgeType::DataAccess
            } else {
                ExecutionEdgeType::SyncCall
            }
        }
        ArchitectureEdgeKind::DataFlow => ExecutionEdgeType::EventEmit,
        ArchitectureEdgeKind::ControlFlow => ExecutionEdgeType::SyncCall,
    }
}
