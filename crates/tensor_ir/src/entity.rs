use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
pub struct EntityId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum EntityType {
    Concept,
    Object,
    Function,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub type_tag: EntityType,
}
