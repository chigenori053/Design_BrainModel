use memory_space_core::{MemoryField, RecallConfig, RecallQuery, RecallResult};
use world_model_core::{ConsistencyScore, Hypothesis, WorldState};

use crate::modality::ModalityInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}

pub type RuntimeResult<T> = Result<T, RuntimeError>;

pub trait MultimodalEncoder {
    fn encode(&self, input: &ModalityInput) -> RuntimeResult<MemoryField>;
}

pub trait MemoryRecallEngine {
    fn recall(&self, query: &RecallQuery, config: RecallConfig) -> RuntimeResult<RecallResult>;
}

pub trait ReasoningEngine {
    fn reason(
        &self,
        world_state: &WorldState,
        recall: Option<&RecallResult>,
    ) -> RuntimeResult<Vec<Hypothesis>>;
}

pub trait DecisionPolicy {
    fn select(&self, hypotheses: &[Hypothesis]) -> RuntimeResult<Option<usize>>;
}

pub trait GeometryEvaluator {
    fn evaluate(&self, field: &MemoryField) -> RuntimeResult<f64>;
}

pub trait LanguageRenderer {
    fn render(
        &self,
        hypotheses: &[Hypothesis],
        evaluation: Option<&ConsistencyScore>,
    ) -> RuntimeResult<String>;
}
