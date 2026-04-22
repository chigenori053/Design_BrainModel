use crate::apply::ApplyResult;
use crate::validate::ValidationResult;

#[derive(Debug, Clone, Default)]
pub struct ExecutionConfig {
    pub dry_run: bool,
    pub max_steps: usize,
}

#[derive(Debug, Clone)]
pub enum ExecutionStep {
    Apply,
    Validate,
    Commit,
    Rollback,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionPlan {
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone)]
pub enum ExecutionResult {
    Applied(ApplyResult),
    Validated(ValidationResult),
    Committed,
    RolledBack,
    Failed(String),
}

pub trait ExecutionEngine {
    fn step(&mut self, step: ExecutionStep) -> ExecutionResult;
}
