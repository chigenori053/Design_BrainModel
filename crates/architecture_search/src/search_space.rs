use architecture_ir::{ArchitectureConstraint, ComponentType};

use crate::grammar::{ComponentRule, ConstraintRule, InterfaceRule, LayerRule};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchSpace {
    pub component_catalog: Vec<ComponentType>,
    pub allowed_dependencies: Vec<DependencyRule>,
    pub constraints: Vec<ArchitectureConstraint>,
    pub forbidden_components: Vec<ComponentType>,
    pub component_rules: Vec<ComponentRule>,
    pub layer_rules: Vec<LayerRule>,
    pub interface_rules: Vec<InterfaceRule>,
    pub constraint_rule: ConstraintRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRule {
    pub from: ComponentType,
    pub to: ComponentType,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignIntent {
    pub required_components: Vec<ComponentType>,
    pub required_features: Vec<String>,
    pub architectural_constraints: Vec<String>,
}
