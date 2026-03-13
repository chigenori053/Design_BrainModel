use architecture_domain::ArchitectureState;

use crate::MathematicalProblem;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericalValidation {
    pub stability_score: f64,
    pub valid: bool,
}

pub trait NumericalValidator {
    fn validate(
        &self,
        architecture: &ArchitectureState,
        problem: &MathematicalProblem,
    ) -> NumericalValidation;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicNumericalValidator;

impl NumericalValidator for DeterministicNumericalValidator {
    fn validate(
        &self,
        architecture: &ArchitectureState,
        problem: &MathematicalProblem,
    ) -> NumericalValidation {
        let component_budget = architecture.metrics.component_count.max(1) as f64 * 4.0;
        let replica_ratio = architecture.deployment.replicas as f64 / component_budget;
        let satisfied_ratio = if problem.constraints.is_empty() {
            1.0
        } else {
            problem.constraints.iter().filter(|constraint| constraint.satisfied).count() as f64
                / problem.constraints.len() as f64
        };
        let stability_score = ((1.0 - replica_ratio.min(1.0)) * 0.4
            + architecture.metrics.layering_score * 0.3
            + satisfied_ratio * 0.3)
            .clamp(0.0, 1.0);
        NumericalValidation {
            stability_score,
            valid: stability_score >= 0.4,
        }
    }
}
