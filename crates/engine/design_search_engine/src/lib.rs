// Phase9-C: DesignSearchEngine (design-state based beam search)
pub mod architecture_cognition;
pub mod constraint;
pub mod design_state;
pub mod engine;
pub mod evaluator;
pub mod hypothesis_graph;
pub mod search_config;
pub mod search_context;
pub mod search_strategy;

// Phase9-D: WorldState-based search with SearchController trait
pub mod architecture_evaluator;
pub mod audit;
pub mod beam_search_controller;
pub mod pruning;
pub mod ranking;
pub mod search_controller;
pub mod search_state;

pub use architecture_cognition::{
    ArchitectureCognitionSearchIntegration, ArchitectureCognitionSnapshot,
};
pub use audit::{
    AccessController, ArchitectureAuditor, AuditContext, AuditCore, AuditDecision, AuditResult,
    AuditTelemetry, AuditTelemetryEvent, CapabilityLimits, FeatureAccess, IntentAuditor,
    PaymentStatus, PlanTier, PolicyCategory, PolicyEnforcement, PolicyEngine, PolicyRegistry,
    PolicyRule, PolicySeverity, SubscriptionController, SubscriptionStatus,
};
pub use constraint::{ConstraintEngine, IntentNode};
pub use design_state::{
    DesignState, DesignStateId, DesignUnit, DesignUnitId, DesignUnitType, EvaluationScore,
};
pub use engine::DesignSearchEngine;
pub use evaluator::Evaluator;
pub use hypothesis_graph::{DesignOperation, DesignTransition, HypothesisGraph};
pub use search_config::SearchConfig;
pub use search_strategy::{BeamSearchStrategy, SearchStrategy};

// Phase9-D exports
pub use architecture_evaluator::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
pub use beam_search_controller::{BeamSearchController, SearchTrace};
pub use design_grammar::{GrammarEngine, GrammarValidation};
pub use pruning::{
    PruneCandidatesOutcome, SearchNodeDiversityPruned, architecture_similarity, prune_candidates,
    prune_candidates_with_telemetry, select_diverse_nodes,
};
pub use ranking::{RankedCandidate, rank_candidates};
pub use search_context::SearchContext;
pub use search_controller::SearchController;
pub use search_state::SearchState;
pub use search_state::SearchState as Phase9SearchState;
pub use world_model_core::{Action, EvaluationVector};
