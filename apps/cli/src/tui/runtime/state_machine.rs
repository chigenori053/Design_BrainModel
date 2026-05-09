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
    Failed,
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
            Self::Failed => "FAILED",
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
