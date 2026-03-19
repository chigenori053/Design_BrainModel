use crate::engine::execution_plan::TestPlan;
use crate::engine::execution_result::StepResult;
use crate::executor::build_executor::run_steps;
use std::path::Path;

pub trait TestExecutor {
    fn test(&self, project_root: &Path, plan: &TestPlan) -> StepResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultTestExecutor;

impl TestExecutor for DefaultTestExecutor {
    fn test(&self, project_root: &Path, plan: &TestPlan) -> StepResult {
        run_steps(project_root, &plan.test_commands, "test")
    }
}
