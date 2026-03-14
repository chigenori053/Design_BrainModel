use std::collections::{BTreeMap, BTreeSet};

use code_ir::CodeIr;
use design_domain::Layer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchitectureNodeKind {
    Module,
    Component,
    Class,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitectureNode {
    pub id: u64,
    pub name: String,
    pub kind: ArchitectureNodeKind,
    pub layer: Layer,
    pub responsibility: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchitectureEdgeKind {
    Dependency,
    DataFlow,
    ControlFlow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitectureEdge {
    pub from: u64,
    pub to: u64,
    pub kind: ArchitectureEdgeKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureGraph {
    pub nodes: Vec<ArchitectureNode>,
    pub edges: Vec<ArchitectureEdge>,
}

impl ArchitectureGraph {
    pub fn dependency_edges(&self) -> impl Iterator<Item = &ArchitectureEdge> {
        self.edges
            .iter()
            .filter(|edge| matches!(edge.kind, ArchitectureEdgeKind::Dependency))
    }

    pub fn layer_map(&self) -> BTreeMap<u64, Layer> {
        self.nodes
            .iter()
            .map(|node| (node.id, node.layer))
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReverseArchitectureReasoner;

impl ReverseArchitectureReasoner {
    pub fn infer_from_code_ir(&self, ir: &CodeIr) -> ArchitectureGraph {
        let nodes = ir
            .modules
            .iter()
            .map(|module| ArchitectureNode {
                id: module.id.0,
                name: module.name.clone(),
                kind: infer_kind(module.name.as_str()),
                layer: module.layer,
                responsibility: module
                    .responsibilities
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unspecified".to_string()),
            })
            .collect::<Vec<_>>();

        let mut edges = ir
            .dependencies
            .iter()
            .map(|dependency| ArchitectureEdge {
                from: dependency.source.0,
                to: dependency.target.0,
                kind: ArchitectureEdgeKind::Dependency,
            })
            .collect::<Vec<_>>();
        edges.extend(ir.control_flow.iter().map(|edge| ArchitectureEdge {
            from: edge.from,
            to: edge.to,
            kind: ArchitectureEdgeKind::ControlFlow,
        }));
        edges.extend(ir.data_flow.iter().map(|edge| ArchitectureEdge {
            from: edge.from,
            to: edge.to,
            kind: ArchitectureEdgeKind::DataFlow,
        }));

        ArchitectureGraph { nodes, edges }
    }

    pub fn responsibilities(&self, graph: &ArchitectureGraph) -> BTreeMap<String, Vec<String>> {
        let mut grouped = BTreeMap::<String, Vec<String>>::new();
        for node in &graph.nodes {
            grouped
                .entry(node.layer.as_str().to_string())
                .or_default()
                .push(node.responsibility.clone());
        }
        grouped
    }

    pub fn modules_by_layer(&self, graph: &ArchitectureGraph) -> BTreeSet<(Layer, String)> {
        graph
            .nodes
            .iter()
            .map(|node| (node.layer, node.name.clone()))
            .collect()
    }
}

fn infer_kind(name: &str) -> ArchitectureNodeKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("module") || lower.contains("service") || lower.contains("repository") {
        ArchitectureNodeKind::Module
    } else if lower.contains("component") || lower.contains("gateway") {
        ArchitectureNodeKind::Component
    } else {
        ArchitectureNodeKind::Class
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_ir::CodeIr;
    use design_domain::DesignUnit;

    #[test]
    fn infers_architecture_graph_from_code_ir() {
        let mut controller = DesignUnit::new(1, "ApiController");
        controller.dependencies.push(design_domain::DesignUnitId(2));
        let service = DesignUnit::new(2, "UserService");

        let graph = ReverseArchitectureReasoner
            .infer_from_code_ir(&CodeIr::from_design_units(&[controller, service]));

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.dependency_edges().count(), 1);
    }
}
