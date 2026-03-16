use serde::{Deserialize, Serialize};

use crate::{LayerId, metrics::ComponentMetrics};

pub type ComponentId = u64;
pub type ComponentUnitId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ComponentNode {
    pub id: ComponentId,
    pub name: String,
    pub component_type: ComponentType,
    #[serde(default)]
    pub layer: Option<LayerId>,
    #[serde(default)]
    pub interfaces: Vec<crate::InterfaceId>,
    #[serde(default)]
    pub properties: Vec<ComponentProperty>,
    pub visibility: Visibility,
    pub metrics: ComponentMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ComponentUnit {
    pub id: ComponentUnitId,
    pub name: String,
    pub component_type: ComponentType,
    #[serde(default)]
    pub layer: Option<LayerId>,
    #[serde(default)]
    pub interfaces: Vec<crate::InterfaceId>,
    #[serde(default)]
    pub properties: Vec<ComponentProperty>,
    pub structures: Vec<crate::StructureUnitId>,
    pub visibility: Visibility,
    pub metrics: ComponentMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ComponentProperty {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ComponentType {
    Module,
    Package,
    Class,
    Struct,
    Trait,
    Interface,
    Function,
    Method,
    Service,
    Repository,
    Controller,
    DataModel,
    Adapter,
    DomainModel,
    UseCase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

impl From<ComponentNode> for ComponentUnit {
    fn from(node: ComponentNode) -> Self {
        Self {
            id: node.id,
            name: node.name,
            component_type: node.component_type,
            layer: node.layer,
            interfaces: node.interfaces,
            properties: node.properties,
            structures: Vec::new(),
            visibility: node.visibility,
            metrics: node.metrics,
        }
    }
}
