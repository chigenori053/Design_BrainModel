use hybrid_vm::{
    ConceptUnitV2, MeaningLayerSnapshotV2, SemanticUnitL1V2, SemanticUnitL2Detail,
};
use design_reasoning::Explanation;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiState {
    Idle,
    Editing,
    Analyzing,
    Reviewing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignTab {
    Concept,
    Specification,
    Item,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    StartEdit,
    Submit,
    AnalysisSucceeded,
    AnalysisFailed,
    #[allow(dead_code)]
    Revise,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    TriggerAnalysis,
}

#[derive(Debug, Clone)]
pub struct TransitionResult {
    #[allow(dead_code)]
    pub next_state: UiState,
    #[allow(dead_code)]
    pub side_effects: Vec<UiEffect>,
    #[allow(dead_code)]
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
    pub concept_text: String,
    pub current_tab: DesignTab,

    pub l1_units: Vec<SemanticUnitL1V2>,
    pub l2_units: Vec<ConceptUnitV2>,
    pub explanation: Option<Explanation>,
    pub snapshot: Option<MeaningLayerSnapshotV2>,

    pub last_error: Option<String>,
    pub ui_state_machine: UiStateMachine,
    #[allow(dead_code)]
    pub graph: CausalGraph,
    #[allow(dead_code)]
    pub graph_positions: HashMap<String, (f32, f32)>,
    #[allow(dead_code)]
    pub selected_node: Option<String>,
    #[allow(dead_code)]
    pub selected_detail: Option<String>,
    #[allow(dead_code)]
    pub edge_builder_from: Option<String>,
    pub cards: Vec<SemanticUnitL2Detail>,
    pub card_edit_buffers: HashMap<String, String>,
    pub missing_info: Vec<hybrid_vm::MissingInfo>,
    pub drafts: Vec<hybrid_vm::DesignDraft>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            concept_text: String::new(),
            current_tab: DesignTab::Concept,
            l1_units: Vec::new(),
            l2_units: Vec::new(),
            explanation: None,
            snapshot: None,
            last_error: None,
            ui_state_machine: UiStateMachine::default(),
            graph: CausalGraph::default(),
            graph_positions: HashMap::new(),
            selected_node: None,
            selected_detail: None,
            edge_builder_from: None,
            cards: Vec::new(),
            card_edit_buffers: HashMap::new(),
            missing_info: Vec::new(),
            drafts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphNodeType {
    L1,
    L2,
    Ghost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphEdgeType {
    Mapping,
    #[allow(dead_code)]
    Causal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub node_type: GraphNodeType,
    pub label: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: GraphEdgeType,
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CausalGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
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
