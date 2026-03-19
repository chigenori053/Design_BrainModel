use crate::dependency::lockfile::write_lockfile;
use crate::dependency::manifest::write_manifest;
use crate::engine::execution_plan::DependencyPlan;
use crate::engine::execution_result::StepResult;
use crate::validation::parse_command;
use std::path::Path;
use std::process::Command;

pub trait DependencyResolver {
    fn resolve(&self, project_root: &Path, plan: &DependencyPlan) -> StepResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultDependencyResolver;

impl DependencyResolver for DefaultDependencyResolver {
    fn resolve(&self, project_root: &Path, plan: &DependencyPlan) -> StepResult {
        if let Err(error) = write_manifest(project_root, plan) {
            return StepResult {
                success: false,
                stdout: String::new(),
                stderr: error,
            };
        }

        let mut all_stdout = String::new();
        let mut all_stderr = String::new();

        for command in &plan.install_commands {
            match run_command(project_root, command) {
                Ok(step) if step.success => {
                    all_stdout.push_str(&step.stdout);
                    all_stderr.push_str(&step.stderr);
                }
                Ok(step) => return step,
                Err(error) => {
                    return StepResult {
                        success: false,
                        stdout: all_stdout,
                        stderr: format!("{}{}", all_stderr, error),
                    };
                }
            }
        }

        if let Err(error) = write_lockfile(project_root, plan) {
            return StepResult {
                success: false,
                stdout: all_stdout,
                stderr: format!("{}{}", all_stderr, error),
            };
        }

        StepResult {
            success: true,
            stdout: all_stdout,
            stderr: all_stderr,
        }
    }
}

pub(crate) fn run_command(project_root: &Path, command: &str) -> Result<StepResult, String> {
    let (program, args) = parse_command(command)?;
    let output = Command::new(program)
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(|e| e.to_string())?;

    Ok(StepResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}
