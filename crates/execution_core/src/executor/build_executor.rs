use crate::dependency::resolver::run_command;
use crate::engine::execution_plan::BuildPlan;
use crate::engine::execution_result::StepResult;
use std::path::Path;

pub trait BuildExecutor {
    fn build(&self, project_root: &Path, plan: &BuildPlan) -> StepResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultBuildExecutor;

impl BuildExecutor for DefaultBuildExecutor {
    fn build(&self, project_root: &Path, plan: &BuildPlan) -> StepResult {
        run_steps(project_root, &plan.build_commands, "build")
    }
}

pub(crate) fn run_steps(project_root: &Path, commands: &[String], phase: &str) -> StepResult {
    if commands.is_empty() {
        return StepResult::skipped(format!("{phase} skipped: no commands"));
    }

    let mut stdout = String::new();
    let mut stderr = String::new();

    for command in commands {
        match run_command(project_root, command) {
            Ok(step) => {
                stdout.push_str(&step.stdout);
                stderr.push_str(&step.stderr);
                if !step.success {
                    return StepResult {
                        success: false,
                        stdout,
                        stderr,
                    };
                }
            }
            Err(error) => {
                stderr.push_str(&error);
                return StepResult {
                    success: false,
                    stdout,
                    stderr,
                };
            }
        }
    }

    StepResult {
        success: true,
        stdout,
        stderr,
    }
}
