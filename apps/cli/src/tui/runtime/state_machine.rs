#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeShellState {
    Idle,
    Analyze,
    Plan,
    Validate,
    Ready,
    PreviewReady,
    AwaitingApply,
    AwaitConfirmation,
    Apply,
    Git,
    Replay,
    BoundedHalt,
    ConvergenceHalt,
    WorldDivergenceHalt,
    VerificationHalt,
    CausalHalt,
    AutonomousRepairHalt,
    ContinuityLossHalt,
    RegressionHalt,
    TopologyCollapseHalt,
    DeploymentDivergenceHalt,
    ExecutionGraphHalt,
    CoordinationCollapseHalt,
    SharedWorldDivergenceHalt,
    DistributedExecutionHalt,
    SemanticContradictionHalt,
    IntentCollapseHalt,
    SemanticReplayHalt,
    SemanticRepairRegressionHalt,
    GovernanceCollapseHalt,
    RunawayCognitionHalt,
    PolicyMutationHalt,
    SemanticGovernanceHalt,
    Rejected,
    GovernanceRejected,
    SemanticRejected,
    ConvergenceRejected,
    MutationSuppressed,
    Failed,
    IntentConvergence,
    ClarificationRequired,
    SemanticAmbiguity,
    FuzzyConvergence,
    IntentCollapse,
    SemanticPlanning,
    IntentDrift,
    SemanticTransition,
    ResponsibilityCollapse,
    PlanningConvergence,
    SemanticDriftRejected,
    // Section 11: Holographic Semantic Memory states
    SemanticMemoryConvergence,
    DuplicateMemoryDetected,
    SemanticAttractorFormation,
    SemanticLineageExpansion,
    AttractorDrift,
    MemoryUniquenessRejected,
    // Section 11: Semantic Abstraction and Concept Synthesis states
    ConceptSynthesis,
    AbstractionCompression,
    ConceptHierarchyFormation,
    CrossDomainTransfer,
    MetaConceptFormation,
    ConceptualDrift,
    SemanticCompressionRejected,
    // Section 10: Long-Horizon Semantic Continuity states
    LongHorizonContinuity,
    TemporalConceptEvolution,
    TemporalAttractorStabilization,
    SemanticDriftRecovery,
    SemanticIdentityCollapse,
    TemporalCompression,
    // Section 10: Semantic World Model Prediction states
    SemanticWorldPrediction,
    FutureTrajectorySimulation,
    ForecastedContradiction,
    PredictiveRepair,
    SemanticFutureCollapse,
    DeploymentPrediction,
    PredictiveConceptEvolution,
    // Section 10: Autonomous Software Evolution states
    AutonomousEvolution,
    AutonomousPlanning,
    SemanticImplementation,
    AutonomousVerification,
    PredictiveRepairExecution,
    DeploymentEvolution,
    DependencyEvolution,
    AutonomousEvolutionCollapse,
    // Section 10: Real Execution Substrate states
    ExecutionTransaction,
    FilesystemMutation,
    GovernedExecution,
    VerificationExecution,
    RollbackRecovery,
    EnvironmentIntegration,
    ExecutionGovernanceHalt,
    PredictiveExecutionReject,
    // Section 10: Cognitive Workspace states
    CognitiveWorkspace,
    ChatInteraction,
    ExecutionProjection,
    GovernanceProjection,
    RollbackProjection,
    IntentClarification,
    ConvergenceProjection,
    // Section 10: Cognitive Workspace Launch Integration states
    WorkspaceInitialization,
    WorkspaceActive,
    ProjectionSynchronization,
    WorkspaceFocusTransition,
    WorkspaceGovernanceReject,
    WorkspaceRecovery,
    // Section 10: Cognitive Workspace TUI v2 - Semantic Observability states
    SemanticObservability,
    ConvergenceVisualization,
    AmbiguityVisualization,
    GovernanceReasoning,
    PredictiveTrajectoryVisualization,
    BranchVisualization,
    CognitiveDensityGovernance,
    // Section 10: Cognitive Focus and Attention Governance states
    AttentionGovernance,
    FocusTransition,
    InterruptSuppression,
    AttentionEscalation,
    ProjectionPrioritization,
    FocusRecovery,
    PredictiveAttentionShift,
    // Section 10: Multi-Branch Cognitive Orchestration states
    MultiBranchCognition,
    BranchArbitration,
    BranchExpansion,
    BranchCollapse,
    BranchPrediction,
    BranchRecovery,
    ConvergenceCompetition,
    // Section 10: Cognitive Temporal Orchestration states
    TemporalCognition,
    TemporalConvergence,
    SemanticAging,
    DelayedContradictionDetection,
    TemporalRecovery,
    TemporalAttentionEscalation,
    TemporalBranchEvolution,
}

impl RuntimeShellState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Analyze => "ANALYZE",
            Self::Plan => "PLAN",
            Self::Validate => "VALIDATE",
            Self::Ready => "READY",
            Self::PreviewReady => "PREVIEW_READY",
            Self::AwaitingApply => "AWAITING_APPLY",
            Self::AwaitConfirmation => "AWAIT_CONFIRMATION",
            Self::Apply => "APPLY",
            Self::Git => "GIT",
            Self::Replay => "REPLAY",
            Self::BoundedHalt => "BOUNDED_HALT",
            Self::ConvergenceHalt => "CONVERGENCE_HALT",
            Self::WorldDivergenceHalt => "WORLD_DIVERGENCE_HALT",
            Self::VerificationHalt => "VERIFICATION_HALT",
            Self::CausalHalt => "CAUSAL_HALT",
            Self::AutonomousRepairHalt => "AUTONOMOUS_REPAIR_HALT",
            Self::ContinuityLossHalt => "CONTINUITY_LOSS_HALT",
            Self::RegressionHalt => "REGRESSION_HALT",
            Self::TopologyCollapseHalt => "TOPOLOGY_COLLAPSE_HALT",
            Self::DeploymentDivergenceHalt => "DEPLOYMENT_DIVERGENCE_HALT",
            Self::ExecutionGraphHalt => "EXECUTION_GRAPH_HALT",
            Self::CoordinationCollapseHalt => "COORDINATION_COLLAPSE_HALT",
            Self::SharedWorldDivergenceHalt => "SHARED_WORLD_DIVERGENCE_HALT",
            Self::DistributedExecutionHalt => "DISTRIBUTED_EXECUTION_HALT",
            Self::SemanticContradictionHalt => "SEMANTIC_CONTRADICTION_HALT",
            Self::IntentCollapseHalt => "INTENT_COLLAPSE_HALT",
            Self::SemanticReplayHalt => "SEMANTIC_REPLAY_HALT",
            Self::SemanticRepairRegressionHalt => "SEMANTIC_REPAIR_REGRESSION_HALT",
            Self::GovernanceCollapseHalt => "GOVERNANCE_COLLAPSE_HALT",
            Self::RunawayCognitionHalt => "RUNAWAY_COGNITION_HALT",
            Self::PolicyMutationHalt => "POLICY_MUTATION_HALT",
            Self::SemanticGovernanceHalt => "SEMANTIC_GOVERNANCE_HALT",
            Self::Rejected => "REJECTED",
            Self::GovernanceRejected => "GOVERNANCE_REJECTED",
            Self::SemanticRejected => "SEMANTIC_REJECTED",
            Self::ConvergenceRejected => "CONVERGENCE_REJECTED",
            Self::MutationSuppressed => "MUTATION_SUPPRESSED",
            Self::Failed => "FAILED",
            Self::IntentConvergence => "INTENT_CONVERGENCE",
            Self::ClarificationRequired => "CLARIFICATION_REQUIRED",
            Self::SemanticAmbiguity => "SEMANTIC_AMBIGUITY",
            Self::FuzzyConvergence => "FUZZY_CONVERGENCE",
            Self::IntentCollapse => "INTENT_COLLAPSE",
            Self::SemanticPlanning => "SEMANTIC_PLANNING",
            Self::IntentDrift => "INTENT_DRIFT",
            Self::SemanticTransition => "SEMANTIC_TRANSITION",
            Self::ResponsibilityCollapse => "RESPONSIBILITY_COLLAPSE",
            Self::PlanningConvergence => "PLANNING_CONVERGENCE",
            Self::SemanticDriftRejected => "SEMANTIC_DRIFT_REJECTED",
            Self::SemanticMemoryConvergence => "SEMANTIC_MEMORY_CONVERGENCE",
            Self::DuplicateMemoryDetected => "DUPLICATE_MEMORY_DETECTED",
            Self::SemanticAttractorFormation => "SEMANTIC_ATTRACTOR_FORMATION",
            Self::SemanticLineageExpansion => "SEMANTIC_LINEAGE_EXPANSION",
            Self::AttractorDrift => "ATTRACTOR_DRIFT",
            Self::MemoryUniquenessRejected => "MEMORY_UNIQUENESS_REJECTED",
            Self::ConceptSynthesis => "CONCEPT_SYNTHESIS",
            Self::AbstractionCompression => "ABSTRACTION_COMPRESSION",
            Self::ConceptHierarchyFormation => "CONCEPT_HIERARCHY_FORMATION",
            Self::CrossDomainTransfer => "CROSS_DOMAIN_TRANSFER",
            Self::MetaConceptFormation => "META_CONCEPT_FORMATION",
            Self::ConceptualDrift => "CONCEPTUAL_DRIFT",
            Self::SemanticCompressionRejected => "SEMANTIC_COMPRESSION_REJECTED",
            Self::LongHorizonContinuity => "LONG_HORIZON_CONTINUITY",
            Self::TemporalConceptEvolution => "TEMPORAL_CONCEPT_EVOLUTION",
            Self::TemporalAttractorStabilization => "TEMPORAL_ATTRACTOR_STABILIZATION",
            Self::SemanticDriftRecovery => "SEMANTIC_DRIFT_RECOVERY",
            Self::SemanticIdentityCollapse => "SEMANTIC_IDENTITY_COLLAPSE",
            Self::TemporalCompression => "TEMPORAL_COMPRESSION",
            Self::SemanticWorldPrediction => "SEMANTIC_WORLD_PREDICTION",
            Self::FutureTrajectorySimulation => "FUTURE_TRAJECTORY_SIMULATION",
            Self::ForecastedContradiction => "FORECASTED_CONTRADICTION",
            Self::PredictiveRepair => "PREDICTIVE_REPAIR",
            Self::SemanticFutureCollapse => "SEMANTIC_FUTURE_COLLAPSE",
            Self::DeploymentPrediction => "DEPLOYMENT_PREDICTION",
            Self::PredictiveConceptEvolution => "PREDICTIVE_CONCEPT_EVOLUTION",
            Self::AutonomousEvolution => "AUTONOMOUS_EVOLUTION",
            Self::AutonomousPlanning => "AUTONOMOUS_PLANNING",
            Self::SemanticImplementation => "SEMANTIC_IMPLEMENTATION",
            Self::AutonomousVerification => "AUTONOMOUS_VERIFICATION",
            Self::PredictiveRepairExecution => "PREDICTIVE_REPAIR_EXECUTION",
            Self::DeploymentEvolution => "DEPLOYMENT_EVOLUTION",
            Self::DependencyEvolution => "DEPENDENCY_EVOLUTION",
            Self::AutonomousEvolutionCollapse => "AUTONOMOUS_EVOLUTION_COLLAPSE",
            Self::ExecutionTransaction => "EXECUTION_TRANSACTION",
            Self::FilesystemMutation => "FILESYSTEM_MUTATION",
            Self::GovernedExecution => "GOVERNED_EXECUTION",
            Self::VerificationExecution => "VERIFICATION_EXECUTION",
            Self::RollbackRecovery => "ROLLBACK_RECOVERY",
            Self::EnvironmentIntegration => "ENVIRONMENT_INTEGRATION",
            Self::ExecutionGovernanceHalt => "EXECUTION_GOVERNANCE_HALT",
            Self::PredictiveExecutionReject => "PREDICTIVE_EXECUTION_REJECT",
            Self::CognitiveWorkspace => "COGNITIVE_WORKSPACE",
            Self::ChatInteraction => "CHAT_INTERACTION",
            Self::ExecutionProjection => "EXECUTION_PROJECTION",
            Self::GovernanceProjection => "GOVERNANCE_PROJECTION",
            Self::RollbackProjection => "ROLLBACK_PROJECTION",
            Self::IntentClarification => "INTENT_CLARIFICATION",
            Self::ConvergenceProjection => "CONVERGENCE_PROJECTION",
            Self::WorkspaceInitialization => "WORKSPACE_INITIALIZATION",
            Self::WorkspaceActive => "WORKSPACE_ACTIVE",
            Self::ProjectionSynchronization => "PROJECTION_SYNCHRONIZATION",
            Self::WorkspaceFocusTransition => "WORKSPACE_FOCUS_TRANSITION",
            Self::WorkspaceGovernanceReject => "WORKSPACE_GOVERNANCE_REJECT",
            Self::WorkspaceRecovery => "WORKSPACE_RECOVERY",
            Self::SemanticObservability => "SEMANTIC_OBSERVABILITY",
            Self::ConvergenceVisualization => "CONVERGENCE_VISUALIZATION",
            Self::AmbiguityVisualization => "AMBIGUITY_VISUALIZATION",
            Self::GovernanceReasoning => "GOVERNANCE_REASONING",
            Self::PredictiveTrajectoryVisualization => "PREDICTIVE_TRAJECTORY_VISUALIZATION",
            Self::BranchVisualization => "BRANCH_VISUALIZATION",
            Self::CognitiveDensityGovernance => "COGNITIVE_DENSITY_GOVERNANCE",
            Self::AttentionGovernance => "ATTENTION_GOVERNANCE",
            Self::FocusTransition => "FOCUS_TRANSITION",
            Self::InterruptSuppression => "INTERRUPT_SUPPRESSION",
            Self::AttentionEscalation => "ATTENTION_ESCALATION",
            Self::ProjectionPrioritization => "PROJECTION_PRIORITIZATION",
            Self::FocusRecovery => "FOCUS_RECOVERY",
            Self::PredictiveAttentionShift => "PREDICTIVE_ATTENTION_SHIFT",
            Self::MultiBranchCognition => "MULTI_BRANCH_COGNITION",
            Self::BranchArbitration => "BRANCH_ARBITRATION",
            Self::BranchExpansion => "BRANCH_EXPANSION",
            Self::BranchCollapse => "BRANCH_COLLAPSE",
            Self::BranchPrediction => "BRANCH_PREDICTION",
            Self::BranchRecovery => "BRANCH_RECOVERY",
            Self::ConvergenceCompetition => "CONVERGENCE_COMPETITION",
            Self::TemporalCognition => "TEMPORAL_COGNITION",
            Self::TemporalConvergence => "TEMPORAL_CONVERGENCE",
            Self::SemanticAging => "SEMANTIC_AGING",
            Self::DelayedContradictionDetection => "DELAYED_CONTRADICTION_DETECTION",
            Self::TemporalRecovery => "TEMPORAL_RECOVERY",
            Self::TemporalAttentionEscalation => "TEMPORAL_ATTENTION_ESCALATION",
            Self::TemporalBranchEvolution => "TEMPORAL_BRANCH_EVOLUTION",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Idle, Self::Analyze)
                | (Self::Analyze, Self::Plan)
                | (Self::Analyze, Self::Failed)
                | (Self::Plan, Self::Validate)
                | (Self::Plan, Self::Failed)
                | (Self::Validate, Self::Ready)
                | (Self::Validate, Self::Failed)
                | (Self::Idle, Self::PreviewReady)
                | (Self::PreviewReady, Self::AwaitingApply)
                | (Self::AwaitingApply, Self::Apply)
                | (Self::AwaitingApply, Self::Idle)
                | (Self::Ready, Self::AwaitConfirmation)
                | (Self::Ready, Self::Idle)
                | (Self::AwaitConfirmation, Self::Apply)
                | (Self::AwaitConfirmation, Self::Idle)
                | (Self::Apply, Self::Git)
                | (Self::Apply, Self::Idle)
                | (Self::Apply, Self::Failed)
                | (Self::Git, Self::Idle)
                | (Self::Git, Self::Failed)
                | (Self::Idle, Self::Replay)
                | (Self::Replay, Self::Idle)
                | (Self::Replay, Self::Failed)
                | (Self::Failed, Self::Idle)
                | (_, Self::BoundedHalt)
                | (Self::BoundedHalt, Self::Idle)
                | (_, Self::ConvergenceHalt)
                | (Self::ConvergenceHalt, Self::Idle)
                | (_, Self::WorldDivergenceHalt)
                | (Self::WorldDivergenceHalt, Self::Idle)
                | (_, Self::VerificationHalt)
                | (Self::VerificationHalt, Self::Idle)
                | (_, Self::CausalHalt)
                | (Self::CausalHalt, Self::Idle)
                | (_, Self::AutonomousRepairHalt)
                | (Self::AutonomousRepairHalt, Self::Idle)
                | (_, Self::ContinuityLossHalt)
                | (Self::ContinuityLossHalt, Self::Idle)
                | (_, Self::RegressionHalt)
                | (Self::RegressionHalt, Self::Idle)
                | (_, Self::TopologyCollapseHalt)
                | (Self::TopologyCollapseHalt, Self::Idle)
                | (_, Self::DeploymentDivergenceHalt)
                | (Self::DeploymentDivergenceHalt, Self::Idle)
                | (_, Self::ExecutionGraphHalt)
                | (Self::ExecutionGraphHalt, Self::Idle)
                | (_, Self::CoordinationCollapseHalt)
                | (Self::CoordinationCollapseHalt, Self::Idle)
                | (_, Self::SharedWorldDivergenceHalt)
                | (Self::SharedWorldDivergenceHalt, Self::Idle)
                | (_, Self::DistributedExecutionHalt)
                | (Self::DistributedExecutionHalt, Self::Idle)
                | (_, Self::SemanticContradictionHalt)
                | (Self::SemanticContradictionHalt, Self::Idle)
                | (_, Self::IntentCollapseHalt)
                | (Self::IntentCollapseHalt, Self::Idle)
                | (_, Self::SemanticReplayHalt)
                | (Self::SemanticReplayHalt, Self::Idle)
                | (_, Self::SemanticRepairRegressionHalt)
                | (Self::SemanticRepairRegressionHalt, Self::Idle)
                | (_, Self::GovernanceCollapseHalt)
                | (Self::GovernanceCollapseHalt, Self::Idle)
                | (_, Self::RunawayCognitionHalt)
                | (Self::RunawayCognitionHalt, Self::Idle)
                | (_, Self::PolicyMutationHalt)
                | (Self::PolicyMutationHalt, Self::Idle)
                | (_, Self::SemanticGovernanceHalt)
                | (Self::SemanticGovernanceHalt, Self::Idle)
                | (_, Self::Rejected)
                | (Self::Rejected, Self::Idle)
                | (_, Self::GovernanceRejected)
                | (Self::GovernanceRejected, Self::Idle)
                | (_, Self::SemanticRejected)
                | (Self::SemanticRejected, Self::Idle)
                | (_, Self::ConvergenceRejected)
                | (Self::ConvergenceRejected, Self::Idle)
                | (_, Self::MutationSuppressed)
                | (Self::MutationSuppressed, Self::Idle)
                | (_, Self::IntentConvergence)
                | (Self::IntentConvergence, Self::Idle)
                | (_, Self::ClarificationRequired)
                | (Self::ClarificationRequired, Self::Idle)
                | (_, Self::SemanticAmbiguity)
                | (Self::SemanticAmbiguity, Self::Idle)
                | (_, Self::FuzzyConvergence)
                | (Self::FuzzyConvergence, Self::Idle)
                | (_, Self::IntentCollapse)
                | (Self::IntentCollapse, Self::Idle)
                | (_, Self::SemanticPlanning)
                | (Self::SemanticPlanning, Self::Idle)
                | (_, Self::IntentDrift)
                | (Self::IntentDrift, Self::Idle)
                | (_, Self::SemanticTransition)
                | (Self::SemanticTransition, Self::Idle)
                | (_, Self::ResponsibilityCollapse)
                | (Self::ResponsibilityCollapse, Self::Idle)
                | (_, Self::PlanningConvergence)
                | (Self::PlanningConvergence, Self::Idle)
                | (_, Self::SemanticDriftRejected)
                | (Self::SemanticDriftRejected, Self::Idle)
                | (_, Self::SemanticMemoryConvergence)
                | (Self::SemanticMemoryConvergence, Self::Idle)
                | (_, Self::DuplicateMemoryDetected)
                | (Self::DuplicateMemoryDetected, Self::Idle)
                | (_, Self::SemanticAttractorFormation)
                | (Self::SemanticAttractorFormation, Self::Idle)
                | (_, Self::SemanticLineageExpansion)
                | (Self::SemanticLineageExpansion, Self::Idle)
                | (_, Self::AttractorDrift)
                | (Self::AttractorDrift, Self::Idle)
                | (_, Self::MemoryUniquenessRejected)
                | (Self::MemoryUniquenessRejected, Self::Idle)
                | (_, Self::ConceptSynthesis)
                | (Self::ConceptSynthesis, Self::Idle)
                | (_, Self::AbstractionCompression)
                | (Self::AbstractionCompression, Self::Idle)
                | (_, Self::ConceptHierarchyFormation)
                | (Self::ConceptHierarchyFormation, Self::Idle)
                | (_, Self::CrossDomainTransfer)
                | (Self::CrossDomainTransfer, Self::Idle)
                | (_, Self::MetaConceptFormation)
                | (Self::MetaConceptFormation, Self::Idle)
                | (_, Self::ConceptualDrift)
                | (Self::ConceptualDrift, Self::Idle)
                | (_, Self::SemanticCompressionRejected)
                | (Self::SemanticCompressionRejected, Self::Idle)
                | (_, Self::LongHorizonContinuity)
                | (Self::LongHorizonContinuity, Self::Idle)
                | (_, Self::TemporalConceptEvolution)
                | (Self::TemporalConceptEvolution, Self::Idle)
                | (_, Self::TemporalAttractorStabilization)
                | (Self::TemporalAttractorStabilization, Self::Idle)
                | (_, Self::SemanticDriftRecovery)
                | (Self::SemanticDriftRecovery, Self::Idle)
                | (_, Self::SemanticIdentityCollapse)
                | (Self::SemanticIdentityCollapse, Self::Idle)
                | (_, Self::TemporalCompression)
                | (Self::TemporalCompression, Self::Idle)
                | (_, Self::SemanticWorldPrediction)
                | (Self::SemanticWorldPrediction, Self::Idle)
                | (_, Self::FutureTrajectorySimulation)
                | (Self::FutureTrajectorySimulation, Self::Idle)
                | (_, Self::ForecastedContradiction)
                | (Self::ForecastedContradiction, Self::Idle)
                | (_, Self::PredictiveRepair)
                | (Self::PredictiveRepair, Self::Idle)
                | (_, Self::SemanticFutureCollapse)
                | (Self::SemanticFutureCollapse, Self::Idle)
                | (_, Self::DeploymentPrediction)
                | (Self::DeploymentPrediction, Self::Idle)
                | (_, Self::PredictiveConceptEvolution)
                | (Self::PredictiveConceptEvolution, Self::Idle)
                | (_, Self::ExecutionTransaction)
                | (Self::ExecutionTransaction, Self::Idle)
                | (_, Self::FilesystemMutation)
                | (Self::FilesystemMutation, Self::Idle)
                | (_, Self::GovernedExecution)
                | (Self::GovernedExecution, Self::Idle)
                | (_, Self::VerificationExecution)
                | (Self::VerificationExecution, Self::Idle)
                | (_, Self::RollbackRecovery)
                | (Self::RollbackRecovery, Self::Idle)
                | (_, Self::EnvironmentIntegration)
                | (Self::EnvironmentIntegration, Self::Idle)
                | (_, Self::ExecutionGovernanceHalt)
                | (Self::ExecutionGovernanceHalt, Self::Idle)
                | (_, Self::PredictiveExecutionReject)
                | (Self::PredictiveExecutionReject, Self::Idle)
                | (_, Self::CognitiveWorkspace)
                | (Self::CognitiveWorkspace, Self::Idle)
                | (_, Self::ChatInteraction)
                | (Self::ChatInteraction, Self::Idle)
                | (_, Self::ExecutionProjection)
                | (Self::ExecutionProjection, Self::Idle)
                | (_, Self::GovernanceProjection)
                | (Self::GovernanceProjection, Self::Idle)
                | (_, Self::RollbackProjection)
                | (Self::RollbackProjection, Self::Idle)
                | (_, Self::IntentClarification)
                | (Self::IntentClarification, Self::Idle)
                | (_, Self::ConvergenceProjection)
                | (Self::ConvergenceProjection, Self::Idle)
                | (_, Self::WorkspaceInitialization)
                | (Self::WorkspaceInitialization, Self::Idle)
                | (_, Self::WorkspaceActive)
                | (Self::WorkspaceActive, Self::Idle)
                | (_, Self::ProjectionSynchronization)
                | (Self::ProjectionSynchronization, Self::Idle)
                | (_, Self::WorkspaceFocusTransition)
                | (Self::WorkspaceFocusTransition, Self::Idle)
                | (_, Self::WorkspaceGovernanceReject)
                | (Self::WorkspaceGovernanceReject, Self::Idle)
                | (_, Self::WorkspaceRecovery)
                | (Self::WorkspaceRecovery, Self::Idle)
                | (_, Self::SemanticObservability)
                | (Self::SemanticObservability, Self::Idle)
                | (_, Self::ConvergenceVisualization)
                | (Self::ConvergenceVisualization, Self::Idle)
                | (_, Self::AmbiguityVisualization)
                | (Self::AmbiguityVisualization, Self::Idle)
                | (_, Self::GovernanceReasoning)
                | (Self::GovernanceReasoning, Self::Idle)
                | (_, Self::PredictiveTrajectoryVisualization)
                | (Self::PredictiveTrajectoryVisualization, Self::Idle)
                | (_, Self::BranchVisualization)
                | (Self::BranchVisualization, Self::Idle)
                | (_, Self::CognitiveDensityGovernance)
                | (Self::CognitiveDensityGovernance, Self::Idle)
                | (_, Self::AttentionGovernance)
                | (Self::AttentionGovernance, Self::Idle)
                | (_, Self::FocusTransition)
                | (Self::FocusTransition, Self::Idle)
                | (_, Self::InterruptSuppression)
                | (Self::InterruptSuppression, Self::Idle)
                | (_, Self::AttentionEscalation)
                | (Self::AttentionEscalation, Self::Idle)
                | (_, Self::ProjectionPrioritization)
                | (Self::ProjectionPrioritization, Self::Idle)
                | (_, Self::FocusRecovery)
                | (Self::FocusRecovery, Self::Idle)
                | (_, Self::PredictiveAttentionShift)
                | (Self::PredictiveAttentionShift, Self::Idle)
                | (_, Self::MultiBranchCognition)
                | (Self::MultiBranchCognition, Self::Idle)
                | (_, Self::BranchArbitration)
                | (Self::BranchArbitration, Self::Idle)
                | (_, Self::BranchExpansion)
                | (Self::BranchExpansion, Self::Idle)
                | (_, Self::BranchCollapse)
                | (Self::BranchCollapse, Self::Idle)
                | (_, Self::BranchPrediction)
                | (Self::BranchPrediction, Self::Idle)
                | (_, Self::BranchRecovery)
                | (Self::BranchRecovery, Self::Idle)
                | (_, Self::ConvergenceCompetition)
                | (Self::ConvergenceCompetition, Self::Idle)
                | (_, Self::TemporalCognition)
                | (Self::TemporalCognition, Self::Idle)
                | (_, Self::TemporalConvergence)
                | (Self::TemporalConvergence, Self::Idle)
                | (_, Self::SemanticAging)
                | (Self::SemanticAging, Self::Idle)
                | (_, Self::DelayedContradictionDetection)
                | (Self::DelayedContradictionDetection, Self::Idle)
                | (_, Self::TemporalRecovery)
                | (Self::TemporalRecovery, Self::Idle)
                | (_, Self::TemporalAttentionEscalation)
                | (Self::TemporalAttentionEscalation, Self::Idle)
                | (_, Self::TemporalBranchEvolution)
                | (Self::TemporalBranchEvolution, Self::Idle)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateTransitionError {
    pub from: RuntimeShellState,
    pub to: RuntimeShellState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateMachine {
    pub current: RuntimeShellState,
}

impl Default for RuntimeStateMachine {
    fn default() -> Self {
        Self {
            current: RuntimeShellState::Idle,
        }
    }
}

impl RuntimeStateMachine {
    pub fn transition_to(
        &mut self,
        next: RuntimeShellState,
    ) -> Result<(), RuntimeStateTransitionError> {
        if self.current == next || self.current.can_transition_to(next) {
            self.current = next;
            Ok(())
        } else {
            Err(RuntimeStateTransitionError {
                from: self.current,
                to: next,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_phase2a_transitions() {
        let mut machine = RuntimeStateMachine::default();

        machine.transition_to(RuntimeShellState::Analyze).unwrap();
        machine.transition_to(RuntimeShellState::Plan).unwrap();
        machine.transition_to(RuntimeShellState::Validate).unwrap();
        machine.transition_to(RuntimeShellState::Ready).unwrap();
        machine
            .transition_to(RuntimeShellState::AwaitConfirmation)
            .unwrap();
        machine.transition_to(RuntimeShellState::Apply).unwrap();
        machine.transition_to(RuntimeShellState::Git).unwrap();
        machine.transition_to(RuntimeShellState::Idle).unwrap();
    }

    #[test]
    fn rejects_forbidden_transition() {
        let mut machine = RuntimeStateMachine {
            current: RuntimeShellState::Plan,
        };

        assert!(machine.transition_to(RuntimeShellState::Apply).is_err());
    }
}
