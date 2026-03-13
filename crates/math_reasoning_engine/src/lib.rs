pub mod complexity_estimator;
pub mod constraint_solver;
pub mod math_telemetry;
pub mod numerical_validator;
pub mod symbolic_reasoning;

use architecture_domain::ArchitectureState;

pub use complexity_estimator::{ComplexityClass, ComplexityEstimate, ComplexityEstimator, HeuristicComplexityEstimator};
pub use constraint_solver::{ConstraintSolution, ConstraintSolver, DeterministicConstraintSolver, MathConstraint, MathVariable};
pub use math_telemetry::{ConstraintSolverTrace, MathReasoningTelemetryEvent, MathReasoningTrace};
pub use numerical_validator::{DeterministicNumericalValidator, NumericalValidation, NumericalValidator};
pub use symbolic_reasoning::{DeterministicSymbolicReasoner, SymbolicReasoner, SymbolicReasoningResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathProblemType {
    ArchitectureValidation,
    ResourceConstraint,
    DependencyConstraint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MathematicalProblem {
    pub problem_type: MathProblemType,
    pub variables: Vec<MathVariable>,
    pub constraints: Vec<MathConstraint>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MathematicalResult {
    pub validity_score: f32,
    pub complexity_estimate: ComplexityEstimate,
    pub constraint_satisfied: bool,
    pub symbolic_summary: String,
    pub numerical_stability: f32,
}

pub trait MathematicalReasoningEngine {
    fn analyze(&self, architecture: &ArchitectureState) -> MathematicalResult;
    fn analyze_with_trace(&self, architecture: &ArchitectureState) -> MathReasoningTrace;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultMathematicalReasoningEngine {
    pub constraint_solver: DeterministicConstraintSolver,
    pub complexity_estimator: HeuristicComplexityEstimator,
    pub numerical_validator: DeterministicNumericalValidator,
    pub symbolic_reasoner: DeterministicSymbolicReasoner,
}

impl DefaultMathematicalReasoningEngine {
    pub fn problem_from_architecture(&self, architecture: &ArchitectureState) -> MathematicalProblem {
        let variables = vec![
            MathVariable {
                name: "components".into(),
                value: architecture.metrics.component_count as f64,
            },
            MathVariable {
                name: "dependencies".into(),
                value: architecture.metrics.dependency_count as f64,
            },
            MathVariable {
                name: "replicas".into(),
                value: architecture.deployment.replicas as f64,
            },
            MathVariable {
                name: "layering".into(),
                value: architecture.metrics.layering_score,
            },
        ];
        let mut constraints = architecture
            .constraints
            .iter()
            .flat_map(|constraint| {
                let mut items = Vec::new();
                if let Some(max_design_units) = constraint.max_design_units {
                    items.push(MathConstraint {
                        expression: format!("components <= {max_design_units}"),
                        satisfied: architecture.metrics.component_count <= max_design_units,
                    });
                }
                if let Some(max_dependencies) = constraint.max_dependencies {
                    items.push(MathConstraint {
                        expression: format!("dependencies <= {max_dependencies}"),
                        satisfied: architecture.metrics.dependency_count <= max_dependencies,
                    });
                }
                items
            })
            .collect::<Vec<_>>();
        constraints.push(MathConstraint {
            expression: "replicas <= components * 4".into(),
            satisfied: architecture.deployment.replicas <= architecture.metrics.component_count.max(1) * 4,
        });
        constraints.push(MathConstraint {
            expression: "layering >= 0.25".into(),
            satisfied: architecture.metrics.layering_score >= 0.25,
        });
        MathematicalProblem {
            problem_type: MathProblemType::ArchitectureValidation,
            variables,
            constraints,
        }
    }
}

impl MathematicalReasoningEngine for DefaultMathematicalReasoningEngine {
    fn analyze(&self, architecture: &ArchitectureState) -> MathematicalResult {
        self.analyze_with_trace(architecture).result
    }

    fn analyze_with_trace(&self, architecture: &ArchitectureState) -> MathReasoningTrace {
        let problem = self.problem_from_architecture(architecture);
        let mut telemetry = vec![MathReasoningTelemetryEvent::MathReasoningStarted];
        let constraint_solution = self.constraint_solver.solve(&problem);
        telemetry.push(MathReasoningTelemetryEvent::ConstraintSolved);
        let complexity_estimate = self.complexity_estimator.estimate(architecture);
        telemetry.push(MathReasoningTelemetryEvent::ComplexityEstimated);
        let symbolic = self.symbolic_reasoner.reason(architecture);
        let numerical = self.numerical_validator.validate(architecture, &problem);
        telemetry.push(MathReasoningTelemetryEvent::MathReasoningCompleted);

        let validity_score = (
            (if constraint_solution.satisfied { 1.0 } else { 0.4 })
                + complexity_estimate.score()
                + symbolic.validity_score
                + numerical.stability_score
        ) / 4.0;

        MathReasoningTrace {
            result: MathematicalResult {
                validity_score: validity_score as f32,
                complexity_estimate,
                constraint_satisfied: constraint_solution.satisfied,
                symbolic_summary: symbolic.summary,
                numerical_stability: numerical.stability_score as f32,
            },
            telemetry,
            constraint_trace: ConstraintSolverTrace {
                problem_type: problem.problem_type,
                checked_constraints: problem.constraints.len(),
                satisfied: constraint_solution.satisfied,
            },
        }
    }
}

