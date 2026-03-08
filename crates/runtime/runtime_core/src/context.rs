use memory_space_core::RecallResult;
use world_model_core::{ConsistencyScore, Hypothesis, WorldState};

use crate::event::RuntimeEventBus;
use crate::modality::ModalityInput;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeStage {
    #[default]
    Input,
    Normalize,
    Recall,
    HypothesisGeneration,
    TransitionEvaluation,
    ConsistencyEvaluation,
    Output,
}

#[derive(Debug, Clone, Default)]
pub struct Phase9RuntimeContext {
    pub request_id: RequestId,
    pub modality_input: ModalityInput,
    pub recall_result: Option<RecallResult>,
    pub world_state: Option<WorldState>,
    pub hypotheses: Vec<Hypothesis>,
    pub evaluation: Option<ConsistencyScore>,
    pub stage: RuntimeStage,
    pub event_bus: RuntimeEventBus,
}

impl Phase9RuntimeContext {
    pub fn advance(&mut self, stage: RuntimeStage) {
        self.stage = stage;
    }
}
