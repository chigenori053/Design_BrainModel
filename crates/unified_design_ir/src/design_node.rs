use crate::DesignMetadata;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignNodeId(pub String);

impl From<&str> for DesignNodeId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for DesignNodeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum DesignNodeKind {
    Module,
    Service,
    API,
    Database,
    Domain,
    Interface,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DesignNode {
    pub id: DesignNodeId,
    pub kind: DesignNodeKind,
    pub name: String,
    pub metadata: DesignMetadata,
}
