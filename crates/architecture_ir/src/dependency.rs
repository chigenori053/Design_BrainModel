use serde::{Deserialize, Serialize};

use crate::{ComponentUnitId, DomainUnitId, InterfaceId, StructureUnitId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DependencyEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub dependency_type: DependencyType,
    #[serde(default)]
    pub interface: Option<InterfaceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum DependencyType {
    Import,
    Call,
    Inherit,
    Implement,
    Use,
    DataFlow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum NodeId {
    Domain(DomainUnitId),
    Component(ComponentUnitId),
    Structure(StructureUnitId),
}
