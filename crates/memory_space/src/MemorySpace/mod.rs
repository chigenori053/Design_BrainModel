pub mod architecture_memory;
pub mod evaluation_memory;
pub mod graph;
pub mod index;
pub mod reasoning_trace_memory;
pub mod space;
pub mod template_memory;
pub mod types;

pub use architecture_memory::{ArchitectureMemoryDomain, ArchitectureMetadata, ArchitectureRecord};
pub use evaluation_memory::{
    EvaluationDiagnostics, EvaluationMemoryDomain, EvaluationMetricsV2, EvaluationRecord,
    EvaluationScores,
};
pub use graph::MemoryGraph;
pub use index::MemoryIndex;
pub use reasoning_trace_memory::{ReasoningTrace, ReasoningTraceMemoryDomain, SearchStep};
pub use space::{DesignMemorySpace, TemplateLearningEvent};
pub use template_memory::{
    DependencyRuleRecord, TemplateMemoryDomain, TemplateMetadata, TemplateRecord, TopologyType,
};
pub use types::{
    DesignIntentRecord, MemoryEdge, MemoryId, MemoryMetadata, MemoryNode, MemoryType, RelationType,
};
