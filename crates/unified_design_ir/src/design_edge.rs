use crate::DesignNodeId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum DesignRelation {
    DependsOn,
    Calls,
    Owns,
    Implements,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignEdge {
    pub source: DesignNodeId,
    pub target: DesignNodeId,
    pub relation: DesignRelation,
}
