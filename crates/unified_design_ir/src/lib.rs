pub mod builder;
pub mod design_edge;
pub mod design_graph;
pub mod design_metadata;
pub mod design_node;
pub mod mapping;
pub mod query;
pub mod validation;

pub use builder::DesignGraphBuilder;
pub use design_edge::{DesignEdge, DesignRelation};
pub use design_graph::{
    DesignGraph, FieldSpec, ImplementationUnit, InterfaceSpec, MethodSpec, StructSpec, TypeRef,
};
pub use design_metadata::{Constraint, DesignMetadata};
pub use design_node::{DesignNode, DesignNodeId, DesignNodeKind};
pub use mapping::{ArchitectureMapper, DefaultArchitectureMapper};
pub use query::DesignQuery;
pub use validation::{DefaultDesignValidator, DesignValidator};
