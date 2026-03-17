use architecture_ir::stable_v03::{ArchitectureGraph, NodeType, RelationType};

use crate::{
    DesignEdge, DesignGraph, DesignGraphBuilder, DesignMetadata, DesignNode, DesignNodeId,
    DesignNodeKind, DesignRelation,
};

pub trait ArchitectureMapper: Send + Sync {
    fn map(&self, graph: &ArchitectureGraph) -> DesignGraph;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultArchitectureMapper;

impl ArchitectureMapper for DefaultArchitectureMapper {
    fn map(&self, graph: &ArchitectureGraph) -> DesignGraph {
        let mut builder = graph
            .nodes()
            .iter()
            .fold(DesignGraphBuilder::new(), |builder, node| {
                builder.add_node(DesignNode {
                    id: DesignNodeId(node.id.0.clone()),
                    kind: map_node_kind(&node.node_type),
                    name: node.id.0.clone(),
                    metadata: DesignMetadata {
                        language_hint: node.metadata.attributes.get("language_hint").cloned(),
                        constraints: Vec::new(),
                        annotations: node
                            .metadata
                            .attributes
                            .iter()
                            .map(|(key, value)| format!("{key}={value}"))
                            .collect(),
                    },
                })
            });

        for edge in graph.edges() {
            builder = builder.add_edge(DesignEdge {
                source: DesignNodeId(edge.source.0.clone()),
                target: DesignNodeId(edge.target.0.clone()),
                relation: map_relation(&edge.relation),
            });
        }

        builder.build()
    }
}

fn map_node_kind(kind: &NodeType) -> DesignNodeKind {
    match kind {
        NodeType::Service => DesignNodeKind::Service,
        NodeType::Component => DesignNodeKind::Module,
        NodeType::DataStore => DesignNodeKind::Database,
        NodeType::Interface => DesignNodeKind::API,
        NodeType::ExternalSystem => DesignNodeKind::Interface,
        NodeType::Custom(name) if name.eq_ignore_ascii_case("domain") => DesignNodeKind::Domain,
        NodeType::Custom(_) => DesignNodeKind::Module,
    }
}

fn map_relation(relation: &RelationType) -> DesignRelation {
    match relation {
        RelationType::DependsOn => DesignRelation::DependsOn,
        RelationType::Calls => DesignRelation::Calls,
        RelationType::Contains => DesignRelation::Owns,
        RelationType::Reads | RelationType::Writes => DesignRelation::DependsOn,
        RelationType::Custom(_) => DesignRelation::Implements,
    }
}
