use crate::engine::execution_plan::RunPlan;
use crate::engine::execution_result::StepResult;
use crate::executor::build_executor::run_steps;
use std::path::Path;

pub trait RunExecutor {
    fn run(&self, project_root: &Path, plan: &RunPlan) -> StepResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultRunExecutor;

impl RunExecutor for DefaultRunExecutor {
    fn run(&self, project_root: &Path, plan: &RunPlan) -> StepResult {
        run_steps(project_root, &plan.run_commands, "run")
    }
}
