use std::collections::BTreeMap;

pub type MemoryId = u64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryType {
    Template,
    Architecture,
    Concept,
    Evaluation,
    Trace,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryMetadata {
    pub key: String,
    pub label: String,
    pub version: u32,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryNode {
    pub node_id: MemoryId,
    pub node_type: MemoryType,
    pub embedding: Vec<f32>,
    pub metadata: MemoryMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationType {
    Implements,
    DerivedFrom,
    SimilarTo,
    EvaluatedAs,
    DependsOn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEdge {
    pub from: MemoryId,
    pub to: MemoryId,
    pub relation: RelationType,
    pub weight: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignIntentRecord {
    pub intent_id: String,
    pub system_type: String,
    pub requirements: Vec<String>,
    pub constraints: Vec<String>,
}
