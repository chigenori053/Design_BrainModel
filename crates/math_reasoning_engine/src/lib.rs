pub mod complexity_estimator;
pub mod constraint_solver;
pub mod graph_analysis;
pub mod math_telemetry;
pub mod numerical_validator;
pub mod optimization;
pub mod symbolic_reasoning;

use architecture_domain::ArchitectureState;
use design_domain::Architecture;
use world_model::{
    AlgorithmAction, ArchitectureAction, DesignAction, EvaluationScore, WorldModel,
};

pub use complexity_estimator::{ComplexityClass, ComplexityEstimate, ComplexityEstimator, HeuristicComplexityEstimator};
pub use constraint_solver::{ConstraintSolution, ConstraintSolver, DeterministicConstraintSolver, MathConstraint, MathVariable};
pub use graph_analysis::{DeterministicGraphAnalysisEngine, GraphAnalysisEngine, GraphMetrics};
pub use math_telemetry::{ConstraintSolverTrace, MathReasoningTelemetryEvent, MathReasoningTrace};
pub use numerical_validator::{DeterministicNumericalValidator, NumericalValidation, NumericalValidator};
pub use optimization::{DeterministicOptimizationEngine, OptimizationEngine, OptimizationResult};
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
    pub graph_metrics: GraphMetrics,
    pub constraint_satisfied: bool,
    pub optimization_result: OptimizationResult,
    pub symbolic_summary: String,
    pub numerical_stability: f32,
}

pub trait MathematicalReasoningEngine {
    fn analyze(&self, architecture: &ArchitectureState) -> MathematicalResult;
    fn analyze_with_trace(&self, architecture: &ArchitectureState) -> MathReasoningTrace;
}

pub trait MathEngine {
    fn evaluate(&self, world: &WorldModel) -> EvaluationScore;
    fn evaluate_action(&self, world: &WorldModel, action: &DesignAction) -> EvaluationScore;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultMathematicalReasoningEngine {
    pub constraint_solver: DeterministicConstraintSolver,
    pub complexity_estimator: HeuristicComplexityEstimator,
    pub graph_analysis_engine: DeterministicGraphAnalysisEngine,
    pub numerical_validator: DeterministicNumericalValidator,
    pub optimization_engine: DeterministicOptimizationEngine,
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
        let graph_metrics = self.graph_analysis_engine.analyze(architecture);
        let symbolic = self.symbolic_reasoner.reason(architecture);
        let numerical = self.numerical_validator.validate(architecture, &problem);
        let optimization_result = self.optimization_engine.optimize(
            complexity_estimate,
            graph_metrics,
            constraint_solution.satisfied,
        );
        telemetry.push(MathReasoningTelemetryEvent::MathReasoningCompleted);

        let validity_score = (
            (if constraint_solution.satisfied { 1.0 } else { 0.4 })
                + complexity_estimate.score()
                + optimization_result.score
                + symbolic.validity_score
                + numerical.stability_score
        ) / 5.0;

        MathReasoningTrace {
            result: MathematicalResult {
                validity_score: validity_score as f32,
                complexity_estimate,
                graph_metrics,
                constraint_satisfied: constraint_solution.satisfied,
                optimization_result,
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

impl DefaultMathematicalReasoningEngine {
    pub fn evaluate_world(&self, world: &WorldModel) -> EvaluationScore {
        let architecture_state = architecture_state_from_world(world);
        let analysis = self.analyze(&architecture_state);
        let maintainability = ((1.0 - analysis.complexity_estimate.score()) * 0.4
            + (1.0 - (analysis.graph_metrics.cycle_count as f64 * 0.2).clamp(0.0, 1.0)) * 0.6)
            .clamp(0.0, 1.0);

        EvaluationScore {
            performance: analysis.optimization_result.score,
            complexity: (1.0 - analysis.complexity_estimate.score()).clamp(0.0, 1.0),
            maintainability,
            correctness: analysis.validity_score as f64,
        }
    }
}

impl MathEngine for DefaultMathematicalReasoningEngine {
    fn evaluate(&self, world: &WorldModel) -> EvaluationScore {
        self.evaluate_world(world)
    }

    fn evaluate_action(&self, world: &WorldModel, action: &DesignAction) -> EvaluationScore {
        let candidate = world.simulate_action(action);
        self.evaluate_world(&candidate)
    }
}

fn architecture_state_from_world(world: &WorldModel) -> ArchitectureState {
    ArchitectureState::from_architecture(
        &Architecture {
            classes: world.design_state.active_design.classes.clone(),
            dependencies: world.design_state.active_design.dependencies.clone(),
            graph: world.design_state.active_design.graph.clone(),
        },
        world.design_state.constraints.clone(),
    )
}

pub fn candidate_actions(world: &WorldModel) -> Vec<DesignAction> {
    world.generate_actions()
}

pub fn architecture_search_step(
    world: &WorldModel,
    engine: &impl MathEngine,
) -> Vec<(DesignAction, EvaluationScore)> {
    let mut actions = candidate_actions(world);
    actions.sort_by_key(action_order_key);
    actions
        .into_iter()
        .map(|action| {
            let score = engine.evaluate_action(world, &action);
            (action, score)
        })
        .collect()
}

fn action_order_key(action: &DesignAction) -> (u8, u64) {
    match action {
        DesignAction::Architecture(ArchitectureAction::AddComponent { component }) => (0, component.id.0),
        DesignAction::Architecture(ArchitectureAction::RemoveComponent { id }) => (1, id.0),
        DesignAction::Architecture(ArchitectureAction::AddDependency { from, to, .. }) => {
            (2, from.0.saturating_mul(10_000).saturating_add(to.0))
        }
        DesignAction::Architecture(ArchitectureAction::RemoveDependency { from, to }) => {
            (3, from.0.saturating_mul(10_000).saturating_add(to.0))
        }
        DesignAction::Architecture(ArchitectureAction::SplitModule { id }) => (4, id.0),
        DesignAction::Architecture(ArchitectureAction::MergeModule { target, source }) => {
            (5, target.0.saturating_mul(10_000).saturating_add(source.0))
        }
        DesignAction::Code(_) => (6, 0),
        DesignAction::Geometry(_) => (7, 0),
        DesignAction::Algorithm(AlgorithmAction::ChangeAlgorithm { target, .. }) => (8, target.0),
        DesignAction::Algorithm(AlgorithmAction::AdjustParameter { target, .. }) => (9, target.0),
        DesignAction::Algorithm(AlgorithmAction::OptimizeStructure { target }) => (10, target.0),
    }
}
