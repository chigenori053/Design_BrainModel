use crate::stable_v03::Metadata;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum NodeType {
    Service,
    Component,
    DataStore,
    Interface,
    ExternalSystem,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub metadata: Metadata,
}

impl Node {
    pub fn new(id: impl Into<NodeId>, node_type: NodeType) -> Self {
        Self {
            id: id.into(),
            node_type,
            metadata: Metadata::default(),
        }
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}
