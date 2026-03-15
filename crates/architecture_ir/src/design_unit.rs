use serde::{Deserialize, Serialize};

pub type DesignUnitId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DesignUnit {
    pub id: DesignUnitId,
    pub semantic_type: SemanticType,
    pub source: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum SemanticType {
    Variable,
    Expression,
    Operator,
    Literal,
    Identifier,
    Statement,
    TypeReference,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
}
