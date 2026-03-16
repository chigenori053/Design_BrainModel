use serde::{Deserialize, Serialize};

use crate::ComponentUnitId;

pub type InterfaceId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct InterfaceUnit {
    pub id: InterfaceId,
    pub name: String,
    #[serde(default)]
    pub input_types: Vec<String>,
    #[serde(default)]
    pub output_types: Vec<String>,
    pub owner_component: ComponentUnitId,
}
