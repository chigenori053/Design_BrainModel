use serde::{Deserialize, Serialize};

use crate::{
    validate_ir, ArchitectureConstraint, ArchitectureGraph, ArchitectureMetadata, ComponentUnit,
    ComponentUnitId, DependencyEdge, DependencyType, DesignUnit, DomainUnit, InterfaceUnit,
    LayerId, NodeId, StructureUnit, ValidationError,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureIR {
    pub metadata: ArchitectureMetadata,
    pub domains: Vec<DomainUnit>,
    pub components: Vec<ComponentUnit>,
    #[serde(default)]
    pub interfaces: Vec<InterfaceUnit>,
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

    pub fn interface_by_id(&self, id: crate::InterfaceId) -> Option<&InterfaceUnit> {
        self.interfaces.iter().find(|interface| interface.id == id)
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

    pub fn add_component(&mut self, component: ComponentUnit) {
        self.components.push(component);
    }

    pub fn remove_component(&mut self, component_id: ComponentUnitId) {
        self.components
            .retain(|component| component.id != component_id);
        self.interfaces
            .retain(|interface| interface.owner_component != component_id);
        self.dependencies.retain(|edge| {
            edge.source != NodeId::Component(component_id)
                && edge.target != NodeId::Component(component_id)
        });
        for layer in &mut self.layers {
            layer.components.retain(|id| *id != component_id);
        }
    }

    pub fn add_interface(&mut self, interface: InterfaceUnit) {
        if let Some(component) = self
            .components
            .iter_mut()
            .find(|component| component.id == interface.owner_component)
        {
            if !component.interfaces.contains(&interface.id) {
                component.interfaces.push(interface.id);
                component.interfaces.sort_unstable();
            }
        }
        self.interfaces.push(interface);
    }

    pub fn add_dependency(&mut self, dep: DependencyEdge) {
        self.dependencies.push(dep);
    }

    pub fn remove_dependency(&mut self, source: NodeId, target: NodeId) {
        self.dependencies
            .retain(|edge| edge.source != source || edge.target != target);
    }

    pub fn move_layer(&mut self, component_id: ComponentUnitId, layer_id: LayerId) {
        for layer in &mut self.layers {
            layer.components.retain(|id| *id != component_id);
        }
        if let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) {
            layer.components.push(component_id);
            layer.components.sort_unstable();
            layer.components.dedup();
        }
        if let Some(component) = self
            .components
            .iter_mut()
            .find(|component| component.id == component_id)
        {
            component.layer = Some(layer_id);
        }
    }

    pub fn split_component(
        &mut self,
        component_id: ComponentUnitId,
        splits: Vec<ComponentUnit>,
    ) -> Result<(), String> {
        let Some(original) = self.component_by_id(component_id).cloned() else {
            return Err(format!("component {component_id} not found"));
        };
        let incoming = self
            .dependencies
            .iter()
            .filter(|edge| edge.target == NodeId::Component(component_id))
            .cloned()
            .collect::<Vec<_>>();
        let outgoing = self
            .dependencies
            .iter()
            .filter(|edge| edge.source == NodeId::Component(component_id))
            .cloned()
            .collect::<Vec<_>>();

        self.remove_component(component_id);
        for split in &splits {
            self.add_component(split.clone());
            if let Some(layer_id) = original
                .layer
                .or_else(|| self.layer_for_component_id(component_id))
            {
                self.move_layer(split.id, layer_id);
            }
        }
        if let Some(primary) = splits.first() {
            for edge in incoming {
                self.add_dependency(DependencyEdge {
                    target: NodeId::Component(primary.id),
                    ..edge
                });
            }
            for edge in outgoing {
                self.add_dependency(DependencyEdge {
                    source: NodeId::Component(primary.id),
                    ..edge
                });
            }
        }
        Ok(())
    }

    pub fn merge_components(
        &mut self,
        component_ids: &[ComponentUnitId],
        merged: ComponentUnit,
    ) -> Result<(), String> {
        if component_ids.is_empty() {
            return Err("component_ids must not be empty".to_string());
        }
        let target_ids = component_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let layer_id = component_ids
            .iter()
            .find_map(|id| self.layer_for_component_id(*id));

        let mut rewritten = Vec::new();
        for edge in &self.dependencies {
            let source = match edge.source {
                NodeId::Component(id) if target_ids.contains(&id) => NodeId::Component(merged.id),
                other => other,
            };
            let target = match edge.target {
                NodeId::Component(id) if target_ids.contains(&id) => NodeId::Component(merged.id),
                other => other,
            };
            if source != target {
                rewritten.push(DependencyEdge {
                    source,
                    target,
                    dependency_type: edge.dependency_type.clone(),
                    interface: edge.interface,
                });
            }
        }
        for id in component_ids {
            self.remove_component(*id);
        }
        self.add_component(merged.clone());
        if let Some(layer_id) = layer_id {
            self.move_layer(merged.id, layer_id);
        }
        self.dependencies = rewritten;
        Ok(())
    }

    fn layer_for_component_id(&self, component_id: ComponentUnitId) -> Option<LayerId> {
        self.layers
            .iter()
            .find(|layer| layer.components.contains(&component_id))
            .map(|layer| layer.id)
    }
}
