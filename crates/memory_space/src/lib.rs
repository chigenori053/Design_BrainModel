pub mod experience_store;
#[path = "MemorySpace/mod.rs"]
pub mod memory_space;
pub mod pattern_extractor;
pub mod pattern_matcher;
pub mod pattern_store;
pub mod search_prior;
pub mod stable_v03;

pub use experience_store::{DesignExperience, ExperienceStore};
pub use memory_space::space::{embed_architecture, embed_evaluation, embed_intent, embed_template};
pub use memory_space::{
    ArchitectureMemoryDomain, ArchitectureMetadata, ArchitectureRecord, DependencyRuleRecord,
    DesignIntentRecord, DesignMemorySpace, EvaluationDiagnostics, EvaluationMemoryDomain,
    EvaluationMetricsV2, EvaluationRecord, EvaluationScores, MemoryEdge, MemoryGraph, MemoryId,
    MemoryIndex, MemoryMetadata, MemoryNode, MemoryType, ReasoningTrace,
    ReasoningTraceMemoryDomain, RelationType, SearchStep, TemplateLearningEvent,
    TemplateMemoryDomain, TemplateMetadata, TemplateRecord, TopologyType,
};
pub use pattern_extractor::{architecture_hash, extract_pattern, layer_sequence_from_state};
pub use pattern_matcher::{PatternMatch, match_patterns};
pub use pattern_store::{
    DesignPattern, InMemoryMemorySpace, MemorySpace, PatternId, PatternStore,
    store_state_experience,
};
pub use search_prior::SearchPrior;
