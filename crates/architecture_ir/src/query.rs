use crate::{ArchitectureIR, ComponentUnitId, NodeId, StructureUnitId};

impl ArchitectureIR {
    pub fn components(&self) -> &[crate::ComponentUnit] {
        &self.components
    }

    pub fn interfaces(&self) -> &[crate::InterfaceUnit] {
        &self.interfaces
    }

    pub fn component_dependencies(&self, component: ComponentUnitId) -> Vec<ComponentUnitId> {
        let mut dependencies = self
            .dependencies
            .iter()
            .filter_map(|edge| match (edge.source, edge.target) {
                (NodeId::Component(source), NodeId::Component(target)) if source == component => {
                    Some(target)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        dependencies.sort_unstable();
        dependencies.dedup();
        dependencies
    }

    pub fn component_dependents(&self, component: ComponentUnitId) -> Vec<ComponentUnitId> {
        let mut dependents = self
            .dependencies
            .iter()
            .filter_map(|edge| match (edge.source, edge.target) {
                (NodeId::Component(source), NodeId::Component(target)) if target == component => {
                    Some(source)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        dependents.sort_unstable();
        dependents.dedup();
        dependents
    }

    pub fn component_structures(&self, component: ComponentUnitId) -> Vec<StructureUnitId> {
        self.component_by_id(component)
            .map(|component| component.structures.clone())
            .unwrap_or_default()
    }

    pub fn component_interfaces(&self, component: ComponentUnitId) -> Vec<crate::InterfaceId> {
        self.component_by_id(component)
            .map(|component| component.interfaces.clone())
            .unwrap_or_default()
    }
}
