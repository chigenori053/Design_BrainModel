use serde::{Deserialize, Serialize};

use crate::ComponentUnitId;

pub type DomainUnitId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DomainUnit {
    pub id: DomainUnitId,
    pub name: String,
    pub components: Vec<ComponentUnitId>,
}
