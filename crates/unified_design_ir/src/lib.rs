pub mod builder;
pub mod convergence_controller;
pub mod design_edge;
pub mod design_graph;
pub mod design_metadata;
pub mod design_node;
pub mod diff_engine;
pub mod fix_engine;
pub mod issue_engine;
pub mod mapping;
pub mod query;
pub mod validation;
pub mod version_core;

pub use builder::DesignGraphBuilder;
pub use convergence_controller::{
    ConvergenceInput, ConvergenceResult, ConvergenceStatus, IterationTrace, MAX_ITERATIONS,
    converge, is_converged, is_deadlock,
};
pub use design_edge::{DesignEdge, DesignRelation};
pub use design_graph::{
    DesignGraph, FieldSpec, ImplementationUnit, InterfaceSpec, MethodSpec, StructSpec, TypeRef,
};
pub use design_metadata::{Constraint, DesignMetadata};
pub use design_node::{DesignNode, DesignNodeId, DesignNodeKind};
pub use diff_engine::{
    ChangeType, DiffError, DiffResult, DiffSummary, FieldChange, FieldPath, Impact, ImpactReason,
    SemanticDiff, SemanticReason, compute_impact, diff, diff_semantic, diff_structural,
    diff_versions,
};
pub use fix_engine::{
    AppliedFix, FixFailureReason, FixInput, FixReport, FixResult, apply_next_fix,
};
pub use issue_engine::{
    FixAction, FixHint, Issue, IssueError, IssueEvidence, IssueId, IssueInput, IssueReason,
    IssueResult, IssueSummary, IssueType, MAX_FIXES_PER_ITERATION, Priority, Severity,
    detect_issues,
};
pub use mapping::{ArchitectureMapper, DefaultArchitectureMapper};
pub use query::DesignQuery;
pub use validation::{DefaultDesignValidator, DesignValidator};
pub use version_core::{
    ArchitectureSpec, CanonicalDesign, ContextSpec, DataSpec, DesignDocument, DesignHistory,
    DesignVersion, ExecutionSpec, FunctionSpec, InterfaceSpec as VersionInterfaceSpec, Metadata,
    Stage, VersionError, VersionId, VersionStatus, compute_hash, create_version, get_head,
    get_version, init_history, list_versions, normalize,
};
