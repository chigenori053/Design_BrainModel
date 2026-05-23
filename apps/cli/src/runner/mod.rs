pub mod plan_validation;
mod process;
mod resolver;
mod sandbox;
mod types;
mod validation;

pub use resolver::{CommandResolver, resolve_command, set_command_override};
pub use sandbox::{build_command, create_sandbox, detect_target, fixed_env};
pub use types::{
    AllowedCommand, CpuReleaseTelemetry, ExecutionConfig, ExecutionResult, ExecutionTarget,
    ExitStatus, MemoryUsage, OutputMeta, OutputMode, RunnerError, RunnerResult, SandboxGuard,
    SandboxInstance, SandboxKey, SandboxMode, SandboxPolicy, Telemetry, TimeoutConfig,
};

use std::path::Path;

use process::execute_process;
use validation::{
    validate_allowed_paths, validate_args, validate_resolved_command, validate_working_dir,
};

pub fn run_protected(
    config: &ExecutionConfig,
    timeout: &TimeoutConfig,
    policy: &SandboxPolicy,
    project_root: &Path,
    sandbox_mode: SandboxMode,
) -> Result<RunnerResult, RunnerError> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(config, timeout, policy, project_root, sandbox_mode)
    }));
    match result {
        Ok(result) => result.map(|result| RunnerResult::Success(Box::new(result))),
        Err(_) => Ok(RunnerResult::Panic),
    }
}

pub fn run(
    config: &ExecutionConfig,
    timeout: &TimeoutConfig,
    policy: &SandboxPolicy,
    project_root: &Path,
    sandbox_mode: SandboxMode,
) -> Result<ExecutionResult, RunnerError> {
    validate_resolved_command(&config.command)?;
    validate_args(&config.args)?;
    let canonical_root =
        validation::canonical_dir(project_root).map_err(RunnerError::ValidationError)?;
    let working_dir = validate_working_dir(&config.working_dir, &canonical_root)?;
    validate_allowed_paths(policy, &canonical_root, &working_dir)?;

    let mut prepared = config.clone();
    prepared.working_dir = working_dir.display().to_string();
    execute_process(&prepared, timeout, policy, sandbox_mode)
}

#[cfg(test)]
mod tests;
