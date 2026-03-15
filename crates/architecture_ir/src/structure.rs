use serde::{Deserialize, Serialize};

use crate::DesignUnitId;

pub type StructureUnitId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct StructureUnit {
    pub id: StructureUnitId,
    pub name: String,
    pub structure_type: StructureType,
    pub design_units: Vec<DesignUnitId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum StructureType {
    Function,
    Method,
    Struct,
    Trait,
    Interface,
    Module,
}
