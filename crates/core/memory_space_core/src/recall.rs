use crate::{ModalityInput, RecallCandidate};

#[derive(Debug, Clone, PartialEq)]
pub struct RecallQuery {
    pub modality: ModalityInput,
    pub context_vector: Vec<f64>,
    pub query_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallConfig {
    pub top_k: usize,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self { top_k: 3 }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecallResult {
    pub candidates: Vec<RecallCandidate>,
}
