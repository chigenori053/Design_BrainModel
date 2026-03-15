use serde::{Deserialize, Serialize};

use crate::{
    ArchitectureConstraint, ArchitectureGraph, ArchitectureMetadata, ComponentUnit,
    ComponentUnitId, DependencyEdge, DependencyType, DesignUnit, DomainUnit, NodeId, StructureUnit,
    ValidationError, validate_ir,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureIR {
    pub metadata: ArchitectureMetadata,
    pub domains: Vec<DomainUnit>,
    pub components: Vec<ComponentUnit>,
    pub structures: Vec<StructureUnit>,
    pub design_units: Vec<DesignUnit>,
    pub dependencies: Vec<DependencyEdge>,
    pub layers: Vec<crate::Layer>,
    pub constraints: Vec<ArchitectureConstraint>,
}

impl ArchitectureIR {
    pub fn component_by_id(&self, id: ComponentUnitId) -> Option<&ComponentUnit> {
        self.components.iter().find(|component| component.id == id)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let result = validate_ir(self);
        if let Some(error) = result.errors.first() {
            Err(error.clone())
        } else {
            Ok(())
        }
    }

    pub fn to_graph(&self) -> petgraph::graph::DiGraph<NodeId, DependencyType> {
        self.build_graph().graph
    }

    pub fn build_graph(&self) -> ArchitectureGraph {
        let mut graph = petgraph::graph::DiGraph::<NodeId, DependencyType>::new();
        let mut node_index = std::collections::HashMap::new();

        for domain in &self.domains {
            let node = NodeId::Domain(domain.id);
            let index = graph.add_node(node);
            node_index.insert(node, index);
        }

        for component in &self.components {
            let node = NodeId::Component(component.id);
            let index = graph.add_node(node);
            node_index.insert(node, index);
        }

        for structure in &self.structures {
            let node = NodeId::Structure(structure.id);
            let index = graph.add_node(node);
            node_index.insert(node, index);
        }

        for dependency in &self.dependencies {
            if let (Some(source), Some(target)) = (
                node_index.get(&dependency.source),
                node_index.get(&dependency.target),
            ) {
                graph.add_edge(*source, *target, dependency.dependency_type.clone());
            }
        }

        ArchitectureGraph { graph, node_index }
    }
}
