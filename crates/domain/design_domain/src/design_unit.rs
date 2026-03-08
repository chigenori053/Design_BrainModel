#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignUnitId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignUnit {
    pub id: DesignUnitId,
    pub name: String,
}

impl DesignUnit {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id: DesignUnitId(id),
            name: name.into(),
        }
    }
}
