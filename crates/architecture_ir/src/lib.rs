mod analyzer;
mod builder;
mod component;
mod constraint;
mod dependency;
mod design_unit;
mod domain;
mod graph;
mod ir;
mod layer;
mod metadata;
mod metrics;
mod query;
mod structure;
mod validation;

pub use analyzer::{
    AnalysisResult, ArchitectureAnalyzer, ArchitectureMetrics, ArchitectureRisk,
    BasicArchitectureAnalyzer, RiskLevel,
};
pub use builder::ArchitectureIRBuilder;
pub use component::{
    ComponentId, ComponentNode, ComponentType, ComponentUnit, ComponentUnitId, Visibility,
};
pub use constraint::{ArchitectureConstraint, ConstraintType};
pub use dependency::{DependencyEdge, DependencyType, NodeId};
pub use design_unit::{DesignUnit, DesignUnitId, SemanticType, SourceLocation};
pub use domain::{DomainUnit, DomainUnitId};
pub use graph::{ArchitectureGraph, architecture_hash, export_dot};
pub use ir::ArchitectureIR;
pub use layer::{Layer, LayerRule};
pub use metadata::ArchitectureMetadata;
pub use metrics::ComponentMetrics;
pub use structure::{StructureType, StructureUnit, StructureUnitId};
pub use validation::{ValidationError, ValidationResult, ValidationWarning, validate_ir};
