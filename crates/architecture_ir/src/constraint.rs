use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureConstraint {
    pub constraint_type: ConstraintType,
    pub description: String,
    #[serde(default)]
    pub value: Option<ConstraintValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ConstraintValue {
    Integer(u64),
    Boolean(bool),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ConstraintType {
    NoCircularDependency,
    LayerViolation,
    DependencyLimit,
    ComplexityLimit,
}
