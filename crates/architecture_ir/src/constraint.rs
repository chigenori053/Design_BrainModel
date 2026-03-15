use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureConstraint {
    pub constraint_type: ConstraintType,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ConstraintType {
    NoCircularDependency,
    LayerViolation,
    DependencyLimit,
    ComplexityLimit,
}
