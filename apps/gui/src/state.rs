use hybrid_vm::{
    ConceptUnitV2, MeaningLayerSnapshotV2, SemanticUnitL1V2,
};
use design_reasoning::Explanation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiState {
    Idle,
    Editing,
    Analyzing,
    Reviewing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    StartEdit,
    Submit,
    AnalysisSucceeded,
    AnalysisFailed,
    Revise,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    TriggerAnalysis,
}

#[derive(Debug, Clone)]
pub struct TransitionResult {
    pub next_state: UiState,
    pub side_effects: Vec<UiEffect>,
    pub applied: bool,
}

#[derive(Debug, Clone)]
pub struct UiStateMachine {
    state: UiState,
}

impl Default for UiStateMachine {
    fn default() -> Self {
        Self { state: UiState::Idle }
    }
}

impl UiStateMachine {
    pub fn current_state(&self) -> UiState {
        self.state
    }

    pub fn dispatch(&mut self, event: UiEvent) -> TransitionResult {
        let current = self.state;
        let (next, side_effects, applied) = match (current, event) {
            (UiState::Idle, UiEvent::StartEdit) => (UiState::Editing, vec![], true),
            (UiState::Editing, UiEvent::Submit) => (UiState::Analyzing, vec![UiEffect::TriggerAnalysis], true),
            (UiState::Analyzing, UiEvent::AnalysisSucceeded) => (UiState::Reviewing, vec![], true),
            (UiState::Analyzing, UiEvent::AnalysisFailed) => (UiState::Error, vec![], true),
            (UiState::Reviewing, UiEvent::Revise) => (UiState::Editing, vec![], true),
            (UiState::Error, UiEvent::Revise) => (UiState::Editing, vec![], true),
            (_, UiEvent::Reset) => (UiState::Idle, vec![], true),
            _ => (current, vec![], false),
        };
        if applied {
            self.state = next;
        }
        TransitionResult {
            next_state: self.state,
            side_effects,
            applied,
        }
    }
}

pub struct AppState {
    pub input_text: String,

    pub l1_units: Vec<SemanticUnitL1V2>,
    pub l2_units: Vec<ConceptUnitV2>,
    pub explanation: Option<Explanation>,
    pub snapshot: Option<MeaningLayerSnapshotV2>,

    pub last_error: Option<String>,
    pub ui_state_machine: UiStateMachine,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            l1_units: Vec::new(),
            l2_units: Vec::new(),
            explanation: None,
            snapshot: None,
            last_error: None,
            ui_state_machine: UiStateMachine::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UiEvent, UiState, UiStateMachine};

    #[test]
    fn valid_transitions_are_deterministic() {
        let mut sm = UiStateMachine::default();
        assert_eq!(sm.current_state(), UiState::Idle);

        assert!(sm.dispatch(UiEvent::StartEdit).applied);
        assert_eq!(sm.current_state(), UiState::Editing);

        assert!(sm.dispatch(UiEvent::Submit).applied);
        assert_eq!(sm.current_state(), UiState::Analyzing);

        assert!(sm.dispatch(UiEvent::AnalysisSucceeded).applied);
        assert_eq!(sm.current_state(), UiState::Reviewing);

        assert!(sm.dispatch(UiEvent::Revise).applied);
        assert_eq!(sm.current_state(), UiState::Editing);

        assert!(sm.dispatch(UiEvent::Reset).applied);
        assert_eq!(sm.current_state(), UiState::Idle);
    }

    #[test]
    fn invalid_transition_is_rejected_without_state_change() {
        let mut sm = UiStateMachine::default();
        let result = sm.dispatch(UiEvent::Submit);
        assert!(!result.applied);
        assert_eq!(sm.current_state(), UiState::Idle);
    }
}
