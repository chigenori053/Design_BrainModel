use crate::MemoryId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoryCandidate {
    pub memory_id: MemoryId,
    pub resonance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallCandidate {
    pub memory_id: MemoryId,
    pub feature_vector: Vec<f64>,
    pub relevance_score: f64,
}
