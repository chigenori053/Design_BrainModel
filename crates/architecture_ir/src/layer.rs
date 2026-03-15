use serde::{Deserialize, Serialize};

use crate::{ComponentType, ComponentUnitId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Layer {
    pub name: String,
    pub level: u32,
    pub components: Vec<ComponentUnitId>,
    pub allowed_dependencies: Vec<LayerRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct LayerRule {
    pub from: ComponentType,
    pub to: ComponentType,
}
