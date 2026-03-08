use crate::DesignUnit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct StructureUnitId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureUnit {
    pub id: StructureUnitId,
    pub name: String,
    pub design_units: Vec<DesignUnit>,
}

impl StructureUnit {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id: StructureUnitId(id),
            name: name.into(),
            design_units: Vec::new(),
        }
    }
}
