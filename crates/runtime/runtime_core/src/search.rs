#[cfg(feature = "legacy-search")]
compile_error!("legacy-search must not be used in runtime");

pub mod architecture_cognition;
pub mod architecture_evaluator;
pub mod audit;
pub mod beam_search_controller;
pub mod concept_beam;
pub mod concept_config;
pub mod concept_controller;
pub mod concept_heuristic;
pub mod concept_pruning;
pub mod concept_state;
pub mod constraint;
pub mod design_state;
pub mod engine;
pub mod evaluator;
pub mod hypothesis_graph;
pub mod pruning;
pub mod ranking;
pub mod reasoning;
pub mod search_config;
pub mod search_context;
pub mod search_controller;
pub mod search_state;
pub mod search_strategy;
pub mod stable_v03;

pub use architecture_cognition::{
    ArchitectureCognitionSearchIntegration, ArchitectureCognitionSnapshot,
    KnowledgeConstrainedSearchPlan,
};
pub use architecture_evaluator::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
pub use audit::{
    AccessController, ArchitectureAuditor, AuditContext, AuditCore, AuditDecision, AuditResult,
    AuditTelemetry, AuditTelemetryEvent, CapabilityLimits, FeatureAccess, IntentAuditor,
    PaymentStatus, PlanTier, PolicyCategory, PolicyEnforcement, PolicyEngine, PolicyRegistry,
    PolicyRule, PolicySeverity, SubscriptionController, SubscriptionStatus,
};
pub use beam_search_controller::{BeamSearchController, SearchTrace};
pub use concept_config::SearchConfig as ConceptSearchConfig;
pub use concept_controller::SearchController as InternalConceptSearchController;
pub use concept_heuristic::{HeuristicSignal, score};
pub use concept_state::SearchState as ConceptSearchState;
pub use constraint::{ConstraintEngine, IntentNode};
pub use design_state::{
    DesignState, DesignStateId, DesignUnit, DesignUnitId, DesignUnitType, EvaluationScore,
};
pub use engine::DesignSearchEngine;
pub use evaluator::Evaluator;
pub use hypothesis_graph::{DesignOperation, DesignTransition, HypothesisGraph};
pub use pruning::{
    PruneCandidatesOutcome, SearchNodeDiversityPruned, architecture_similarity, prune_candidates,
    prune_candidates_with_telemetry, select_diverse_nodes,
};
pub use ranking::{RankedCandidate, rank_candidates};
pub use reasoning::{
    ArchitectureHypothesis, HypothesisGenerator, HypothesisValidation, IntentGraph, IntentParser,
    KnowledgeRetriever as ReasoningKnowledgeRetriever, ReasoningConfig, ReasoningEngine,
    ReasoningResult, ReasoningTelemetry, ReasoningTelemetryEvent, ReasoningValidator,
    runtime_hypotheses_from_reasoning,
};
pub use search_config::SearchConfig;
pub use search_context::SearchContext;
pub use search_controller::SearchController;
pub use search_state::SearchState;
pub use search_strategy::{BeamSearchStrategy, SearchStrategy};
pub use simulation_scheduler::{
    DefaultSimulationScheduler, KnowledgeScore, LightSimulationResult, LightSimulationTrace,
    ScheduledCandidate, ScheduledSimulationBatch, SchedulerTelemetryEvent,
    SimulationSchedulerConfig, SimulationSchedulerTrace,
};
pub use stable_v03::{
    ArchitectureCandidate, Constraint, Context, ContractRelationType, Decision,
    DesignSearchEngine as StableDesignSearchEngine, DeterministicBeamSearchEngine, Goal,
    Hypothesis, HypothesisAuditSnapshot, HypothesisId, Intent, MemoryCandidate, MemoryRef, NodeId,
    ReasoningCore, ReasoningInput, ReasoningSearchResult, ReasoningTrace, RecallContext,
    RecalledPattern, Relation, RelationId, RequestId, ScoreParts, SemanticHash,
    SemanticRepresentation, State, StateHash, Strategy, StrategyReason, TraceProofStep, TraceStats,
    TraceStep, ValidationReason, ValidationResult, decide, evaluate_hypothesis, request_id_for,
    semantic_hash_for_text, state_hash_for_graph, validate_hypothesis_set,
};
pub use world_model_core::{Action, EvaluationVector};

#[derive(Clone, Debug)]
pub struct ConceptSearchController {
    inner: InternalConceptSearchController,
}

impl ConceptSearchController {
    pub fn new(config: ConceptSearchConfig) -> Self {
        Self {
            inner: InternalConceptSearchController::new(config),
        }
    }

    pub fn config(&self) -> ConceptSearchConfig {
        self.inner.config()
    }

    pub fn search(
        &self,
        initial: ConceptSearchState,
        concepts: &[concept_engine::ConceptId],
        memory: &[memory_space_api::ConceptRecallHit],
        intent_edges: usize,
    ) -> Vec<ConceptSearchState> {
        self.inner.search(initial, concepts, memory, intent_edges)
    }
}

impl Default for ConceptSearchController {
    fn default() -> Self {
        Self::new(ConceptSearchConfig::default())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchInput {
    pub initial_state: world_model_core::WorldState,
    pub recall: Option<memory_space_core::RecallResult>,
    pub config: SearchConfig,
    pub context: SearchContext,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub beam: Vec<SearchState>,
    pub trace: SearchTrace,
}

pub fn search(input: SearchInput) -> SearchResult {
    let controller = BeamSearchController::default();
    let trace = controller.search_trace_with_context(
        input.initial_state,
        input.recall.as_ref(),
        &input.config,
        &input.context,
    );
    SearchResult {
        beam: trace.final_beam.clone(),
        trace,
    }
}
