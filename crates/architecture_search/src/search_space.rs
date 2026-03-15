use architecture_ir::{ArchitectureConstraint, ComponentType};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchSpace {
    pub component_catalog: Vec<ComponentType>,
    pub allowed_dependencies: Vec<DependencyRule>,
    pub constraints: Vec<ArchitectureConstraint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRule {
    pub from: ComponentType,
    pub to: ComponentType,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignIntent {
    pub required_components: Vec<ComponentType>,
}
