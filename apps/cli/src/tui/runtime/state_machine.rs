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
