pub mod hypothesis_engine;
pub mod language_engine;
pub mod meaning_engine;
pub mod phase1_engine;
pub mod projection_engine;
pub mod snapshot_engine;
pub mod structured_reasoning;

pub use hypothesis_engine::{DesignHypothesis, HypothesisEngine};
pub use language_engine::{
    Explanation, LanguageEngine, LanguagePatternStore, LanguageState, LanguageStateV2,
    TEMPLATE_SELECTION_EPSILON, TemplateId, is_ambiguous_margin,
};
pub use meaning_engine::MeaningEngine;
pub use phase1_engine::{
    DependencyConsistencyMetrics, DesignFactor, FactorType, Phase1Engine, SanityStats, ScsInputs,
    compute_dependency_consistency, compute_dependency_consistency_metrics, compute_scs_v1_1,
    sanitize_factors,
};
pub use projection_engine::ProjectionEngine;
pub use snapshot_engine::{MeaningLayerSnapshotV2, SnapshotDiffV2, SnapshotEngine};
pub use structured_reasoning::{
    AxisCategory, IssueType, OverallState, RealizationMode, RealizedExplanation, ReasoningAxis,
    StructuredExplanationResult, StructuredReasoningEngine, StructuredReasoningInput,
    StructuredReasoningTrace, format_explanation, parse_realization_mode_from_env,
};
