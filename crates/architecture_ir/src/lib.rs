mod analyzer;
mod builder;
mod component;
mod constraint;
mod dependency;
mod design_unit;
mod domain;
mod graph;
mod interface;
mod ir;
mod layer;
mod metadata;
mod metrics;
mod query;
pub mod stable_v03;
mod structure;
mod validation;

pub use analyzer::{
    AnalysisResult, ArchitectureAnalyzer, ArchitectureMetrics, ArchitectureRisk,
    BasicArchitectureAnalyzer, RiskLevel,
};
pub use builder::ArchitectureIRBuilder;
pub use component::{
    ComponentId, ComponentNode, ComponentProperty, ComponentType, ComponentUnit, ComponentUnitId,
    Visibility,
};
pub use constraint::{ArchitectureConstraint, ConstraintType, ConstraintValue};
pub use dependency::{DependencyEdge, DependencyType, NodeId};
pub use design_unit::{DesignUnit, DesignUnitId, SemanticType, SourceLocation};
pub use domain::{DomainUnit, DomainUnitId};
pub use graph::{architecture_hash, export_dot, ArchitectureGraph};
pub use interface::{InterfaceId, InterfaceUnit};
pub use ir::ArchitectureIR;
pub use layer::{Layer, LayerId, LayerRule};
pub use metadata::ArchitectureMetadata;
pub use metrics::ComponentMetrics;
pub use structure::{StructureType, StructureUnit, StructureUnitId};
pub use validation::{validate_ir, ValidationError, ValidationResult, ValidationWarning};
