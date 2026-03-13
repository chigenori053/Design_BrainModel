use crate::{MathProblemType, MathematicalResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathReasoningTelemetryEvent {
    MathReasoningStarted,
    ConstraintSolved,
    ComplexityEstimated,
    MathReasoningCompleted,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintSolverTrace {
    pub problem_type: MathProblemType,
    pub checked_constraints: usize,
    pub satisfied: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MathReasoningTrace {
    pub result: MathematicalResult,
    pub telemetry: Vec<MathReasoningTelemetryEvent>,
    pub constraint_trace: ConstraintSolverTrace,
}
