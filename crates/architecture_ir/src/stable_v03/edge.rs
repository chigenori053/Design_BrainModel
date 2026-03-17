use crate::stable_v03::NodeId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum RelationType {
    DependsOn,
    Calls,
    Contains,
    Reads,
    Writes,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Edge {
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationType,
}

impl Edge {
    pub fn new(
        source: impl Into<NodeId>,
        target: impl Into<NodeId>,
        relation: RelationType,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            relation,
        }
    }
}
