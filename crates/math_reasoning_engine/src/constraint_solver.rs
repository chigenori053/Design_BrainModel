use crate::MathematicalProblem;

#[derive(Clone, Debug, PartialEq)]
pub struct MathVariable {
    pub name: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MathConstraint {
    pub expression: String,
    pub satisfied: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintSolution {
    pub satisfied: bool,
    pub satisfied_constraints: usize,
}

pub trait ConstraintSolver {
    fn solve(&self, problem: &MathematicalProblem) -> ConstraintSolution;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicConstraintSolver;

impl ConstraintSolver for DeterministicConstraintSolver {
    fn solve(&self, problem: &MathematicalProblem) -> ConstraintSolution {
        let satisfied_constraints = problem
            .constraints
            .iter()
            .filter(|constraint| constraint.satisfied)
            .count();
        ConstraintSolution {
            satisfied: satisfied_constraints == problem.constraints.len(),
            satisfied_constraints,
        }
    }
}
