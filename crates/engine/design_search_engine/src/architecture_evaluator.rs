use world_model_core::{EvaluationVector, evaluate_architecture};

use crate::search_state::SearchState;

/// Evaluates a design candidate's architectural quality.
pub trait ArchitectureEvaluator {
    fn evaluate(&self, state: &SearchState) -> f64;

    fn evaluate_vector(&self, state: &SearchState) -> EvaluationVector {
        evaluate_architecture(
            &state.world_state.architecture,
            &state.world_state.constraints,
        )
    }
}

/// Phase10 evaluator: score architecture quality with a light depth penalty.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultArchitectureEvaluator;

impl ArchitectureEvaluator for DefaultArchitectureEvaluator {
    fn evaluate(&self, state: &SearchState) -> f64 {
        let depth_decay = 1.0 / (1.0 + state.depth as f64 * 0.1);
        (self.evaluate_vector(state).total() * depth_decay).clamp(0.0, 1.0)
    }

    fn evaluate_vector(&self, state: &SearchState) -> EvaluationVector {
        let mut vector = evaluate_architecture(
            &state.world_state.architecture,
            &state.world_state.constraints,
        );
        if let Some(math) = &state.math_reasoning {
            vector.constraint_satisfaction =
                ((vector.constraint_satisfaction + math.result.validity_score as f64) / 2.0)
                    .clamp(0.0, 1.0);
        }
        if let Some(simulation) = &state.world_state.simulation {
            vector.simulation_quality = simulation.total();
            vector.constraint_satisfaction =
                ((vector.constraint_satisfaction + simulation.constraint_score) / 2.0)
                    .clamp(0.0, 1.0);
        }
        vector
    }
}
