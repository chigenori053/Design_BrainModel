use memory_space_complex::ComplexField;

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningInput {
    pub semantic_vector: ComplexField,
    pub context: Option<ComplexField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hypothesis {
    pub action_vector: ComplexField,
    pub predicted_score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningResult {
    pub solution_vector: ComplexField,
    pub confidence: f64,
    pub stats: ReasoningStats,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningStats {
    pub used_recall: bool,
    pub recall_resonance: f64,
    pub recall_entropy: f64,
    pub hypotheses_generated: usize,
    pub simulation_steps: usize,
    pub evaluation_score: f64,
}

impl Default for ReasoningStats {
    fn default() -> Self {
        Self {
            used_recall: false,
            recall_resonance: 0.0,
            recall_entropy: 0.0,
            hypotheses_generated: 0,
            simulation_steps: 0,
            evaluation_score: 0.0,
        }
    }
}
