pub mod hypothesis_engine;
pub mod language_engine;
pub mod meaning_engine;
pub mod phase1_engine;
pub mod projection_engine;
pub mod snapshot_engine;
pub mod structured_reasoning;
pub mod fuzzy_convergence;
pub mod semantic_planning;
pub mod holographic_semantic_memory;
pub mod semantic_concept_synthesis;
pub mod long_horizon_semantic_continuity;
pub mod semantic_world_prediction;
pub mod autonomous_software_evolution;

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
    AxisCategory, IssueType, ModelConfig, OverallState, RealizationMode, RealizedExplanation,
    ReasoningAxis, SrtIssue, SrtStrength, StructuredExplanationResult, StructuredReasoningEngine,
    StructuredReasoningInput, StructuredReasoningTrace, ValidationError, canonical_srt_hash,
    format_explanation, llm_cache_key, model_version, normalize_realized_explanation_for_output,
    normalize_summary_text, parse_realization_mode_from_env, validate_llm_output,
    validate_sentence_count,
};
pub use fuzzy_convergence::{
    DesignConvergenceEngine, DesignConvergenceScore, FuzzyIntentScore, FuzzyJudgeLogic,
    IntentCandidate, LatentConstraint,
};
pub use semantic_planning::{
    AbstractionTransition, IntentContinuityEngine, IntentLineage, PlanningMemory,
    ResponsibilityUnit, SemanticPlanningEngine, SemanticPlanningGraph, SemanticPlanningNode,
};
pub use holographic_semantic_memory::{
    DuplicateEliminationEngine, GeneralizedAbstraction, GovernanceEvent, HolographicMemoryStore,
    HolographicSemanticMemory, SemanticAttractor, SemanticGeneralizationEngine,
    SemanticIdentityScore, SemanticLineage, UniquenessGovernanceEngine, UniquenessScore,
};
pub use semantic_concept_synthesis::{
    ConceptCognitionRuntime, ConceptFormationEvent, ConceptGraph, ConceptHierarchyNode,
    ConceptNode, ConceptSynthesisEngine, ConceptSynthesisReport, CrossDomainTransferEngine,
    SemanticAbstraction, SemanticAbstractionEngine, SemanticCompressionEngine,
    SemanticPatternInput, TransferMapping,
};
pub use long_horizon_semantic_continuity::{
    ContinuityEvent, DriftClassification, EvolvingConcept, LongHorizonContinuityRuntime,
    LongHorizonPlanningState, LongHorizonReport, SemanticIdentityState, TemporalAttractor,
    TemporalContinuityEngine, TemporalSemanticMemory,
};
pub use semantic_world_prediction::{
    DeploymentForecast, ForecastedContradiction, FutureSemanticTrajectory, PredictionEvent,
    PredictiveConceptEvolution, PredictiveSemanticRepair, SemanticConsequence,
    SemanticWorldPredictionReport, SemanticWorldPredictionRuntime, SemanticWorldState,
    WorldPredictionEngine,
};
pub use autonomous_software_evolution::{
    AutonomousEvolutionEngine, AutonomousEvolutionState, AutonomousPlan,
    AutonomousVerificationResult, DependencyEvolution, DeploymentEvolutionState,
    SemanticImplementationUnit,
};
