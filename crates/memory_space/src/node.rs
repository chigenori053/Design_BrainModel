use std::collections::BTreeMap;

use crate::types::{NodeId, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct DesignNode {
    pub id: NodeId,
    pub kind: String,
    pub attributes: BTreeMap<String, Value>,
}

impl DesignNode {
    pub fn new(id: NodeId, kind: impl Into<String>, attributes: BTreeMap<String, Value>) -> Self {
        Self {
            id,
            kind: kind.into(),
            attributes,
        }
    }

    pub fn with_id(
        id: NodeId,
        kind: impl Into<String>,
        attributes: BTreeMap<String, Value>,
    ) -> Self {
        Self::new(id, kind, attributes)
    }
}
