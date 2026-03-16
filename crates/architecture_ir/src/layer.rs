use serde::{Deserialize, Serialize};

use crate::{ComponentType, ComponentUnitId};

pub type LayerId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Layer {
    #[serde(default)]
    pub id: LayerId,
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
