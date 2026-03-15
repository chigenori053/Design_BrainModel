use crate::{
    ArchitectureConstraint, ArchitectureIR, ArchitectureMetadata, ComponentMetrics, ComponentType,
    ComponentUnit, DependencyEdge, DependencyType, DesignUnit, DomainUnit, Layer, NodeId,
    SourceLocation, StructureType, StructureUnit, Visibility,
};

#[derive(Clone, Debug, Default)]
pub struct ArchitectureIRBuilder {
    ir: ArchitectureIR,
}

impl ArchitectureIRBuilder {
    pub fn new(metadata: ArchitectureMetadata) -> Self {
        Self {
            ir: ArchitectureIR {
                metadata,
                ..ArchitectureIR::default()
            },
        }
    }

    pub fn add_domain(mut self, id: u64, name: impl Into<String>, components: Vec<u64>) -> Self {
        self.ir.domains.push(DomainUnit {
            id,
            name: name.into(),
            components,
        });
        self
    }

    pub fn add_component(
        mut self,
        id: u64,
        name: impl Into<String>,
        component_type: ComponentType,
    ) -> Self {
        self.ir.components.push(ComponentUnit {
            id,
            name: name.into(),
            component_type,
            structures: Vec::new(),
            visibility: Visibility::Public,
            metrics: ComponentMetrics::default(),
        });
        self
    }

    pub fn add_structure(
        mut self,
        id: u64,
        name: impl Into<String>,
        structure_type: StructureType,
    ) -> Self {
        self.ir.structures.push(StructureUnit {
            id,
            name: name.into(),
            structure_type,
            design_units: Vec::new(),
        });
        self
    }

    pub fn add_design_unit(
        mut self,
        id: u64,
        semantic_type: crate::SemanticType,
        file: impl Into<String>,
        line: usize,
    ) -> Self {
        self.ir.design_units.push(DesignUnit {
            id,
            semantic_type,
            source: SourceLocation {
                file: file.into(),
                line,
            },
        });
        self
    }

    pub fn attach_structure_to_component(mut self, component_id: u64, structure_id: u64) -> Self {
        if let Some(component) = self
            .ir
            .components
            .iter_mut()
            .find(|component| component.id == component_id)
        {
            component.structures.push(structure_id);
            component.structures.sort_unstable();
            component.structures.dedup();
        }
        self
    }

    pub fn add_dependency(
        mut self,
        source: NodeId,
        target: NodeId,
        dependency_type: DependencyType,
    ) -> Self {
        self.ir.dependencies.push(DependencyEdge {
            source,
            target,
            dependency_type,
        });
        self
    }

    pub fn add_layer(mut self, layer: Layer) -> Self {
        self.ir.layers.push(layer);
        self
    }

    pub fn add_constraint(mut self, constraint: ArchitectureConstraint) -> Self {
        self.ir.constraints.push(constraint);
        self
    }

    pub fn build(self) -> ArchitectureIR {
        self.ir
    }
}
