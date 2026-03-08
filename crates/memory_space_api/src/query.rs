use memory_space_complex::ComplexField;
use memory_space_core::MemoryId;
use memory_space_eval::RecallScore;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryQuery {
    pub vector: ComplexField,
    pub context: Option<ComplexField>,
    pub k: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoredCandidate {
    pub memory_id: MemoryId,
    pub resonance: f64,
    pub score: f64,
    pub confidence: f64,
    pub ambiguity: f64,
}

impl ScoredCandidate {
    pub fn from_parts(memory_id: MemoryId, resonance: f64, recall: RecallScore) -> Self {
        Self {
            memory_id,
            resonance,
            score: recall.score,
            confidence: recall.confidence,
            ambiguity: recall.ambiguity,
        }
    }
}
