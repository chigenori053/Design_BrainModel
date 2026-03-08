use concept_engine::ConceptId;

use crate::design_state::{DesignState, EvaluationScore};

#[derive(Clone, Copy, Debug, Default)]
pub struct Evaluator;

impl Evaluator {
    pub fn evaluate(&self, state: &DesignState, concepts: &[ConceptId]) -> EvaluationScore {
        let unit_count = state.design_units.len() as f64;
        let structural = (unit_count / 10.0).clamp(0.0, 1.0);

        let dep_edges = state
            .design_units
            .iter()
            .map(|unit| unit.dependencies.len())
            .sum::<usize>() as f64;
        let dependency = if unit_count <= f64::EPSILON {
            0.0
        } else {
            (1.0 - (dep_edges / (unit_count * 4.0))).clamp(0.0, 1.0)
        };

        let concept_alignment = if concepts.is_empty() {
            0.0
        } else {
            (unit_count / concepts.len() as f64).clamp(0.0, 1.0)
        };

        EvaluationScore {
            structural,
            dependency,
            concept_alignment,
        }
    }
}
