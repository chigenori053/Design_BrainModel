use crate::StructureUnit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassUnit {
    pub id: u64,
    pub name: String,
    pub structures: Vec<StructureUnit>,
}

impl ClassUnit {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            structures: Vec::new(),
        }
    }
}
