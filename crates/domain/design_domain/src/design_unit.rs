use crate::Layer;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignUnitId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignUnit {
    pub id: DesignUnitId,
    pub name: String,
    pub layer: Layer,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub dependencies: Vec<DesignUnitId>,
    pub semantics: Vec<String>,
}

impl DesignUnit {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: DesignUnitId(id),
            layer: Layer::infer_from_name(&name),
            name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            dependencies: Vec::new(),
            semantics: Vec::new(),
        }
    }

    pub fn with_layer(id: u64, name: impl Into<String>, layer: Layer) -> Self {
        let mut unit = Self::new(id, name);
        unit.layer = layer;
        unit
    }
}
