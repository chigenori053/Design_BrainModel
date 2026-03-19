use crate::container::container_manager::{ContainerManager, DefaultContainerManager};
use crate::controller::retry_policy::RetryPolicy;
use crate::controller::timeout_policy::TimeoutPolicy;
use crate::determinism::determinism_report::DeterminismReport;
use crate::determinism::determinism_validator::DeterminismValidator;
use crate::environment::filesystem_guard::FilesystemGuard;
use crate::environment::isolation::{EnvironmentManager, IsolatedEnvironmentManager};
use crate::environment::network_guard::{NetworkGuard, NetworkMode};
use crate::environment::workspace::Workspace;
use crate::failure::failure_analyzer::FailureAnalyzer;
use crate::failure::failure_type::FailureType;
use crate::replay::replay_engine::{DefaultReplayEngine, ReplayEngine};
use crate::reproducibility::snapshot::{ExecutionSnapshot, ReproducibilityManager};
use crate::trace::execution_trace::ExecutionTrace;
use crate::trace::step_trace::StepTrace;
use crate::validation::validate_execution_plan;
use execution_core::dependency::lockfile::write_lockfile;
use execution_core::dependency::manifest::write_manifest;
use execution_core::engine::execution_plan::{DependencyPlan, ExecutionPlan};
use execution_core::engine::execution_result::StepResult;
use execution_core::validation::parse_command;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub trait ExecutionController {
    fn execute_with_control(&self, plan: &ExecutionPlan) -> ExecutionResult;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub success: bool,
    pub dependency_result: StepResult,
    pub build_result: StepResult,
    pub run_result: StepResult,
    pub test_result: StepResult,
    pub trace: ExecutionTrace,
    pub failure_type: Option<FailureType>,
    pub snapshot: ExecutionSnapshot,
    pub determinism_report: Option<DeterminismReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConfig {
    pub use_container: bool,
    pub network_mode: NetworkMode,
    pub enable_determinism_check: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            use_container: false,
            network_mode: NetworkMode::Disabled,
            enable_determinism_check: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DefaultExecutionController {
    pub timeout_policy: TimeoutPolicy,
    pub retry_policy: RetryPolicy,
    pub environment_manager: IsolatedEnvironmentManager,
    pub failure_analyzer: FailureAnalyzer,
    pub reproducibility_manager: ReproducibilityManager,
    pub container_manager: DefaultContainerManager,
    pub filesystem_guard: FilesystemGuard,
    pub network_guard: NetworkGuard,
    pub determinism_validator: DeterminismValidator,
    pub replay_engine: DefaultReplayEngine,
    pub config: ExecutionConfig,
    pub dry_run: bool,
    pub stop_on_failure: bool,
}

impl Default for DefaultExecutionController {
    fn default() -> Self {
        Self {
            timeout_policy: TimeoutPolicy::default(),
            retry_policy: RetryPolicy::default(),
            environment_manager: IsolatedEnvironmentManager::default(),
            failure_analyzer: FailureAnalyzer,
            reproducibility_manager: ReproducibilityManager::default(),
            container_manager: DefaultContainerManager::default(),
            filesystem_guard: FilesystemGuard,
            network_guard: NetworkGuard::default(),
            determinism_validator: DeterminismValidator,
            replay_engine: DefaultReplayEngine::default(),
            config: ExecutionConfig::default(),
            dry_run: false,
            stop_on_failure: true,
        }
    }
}

impl ExecutionController for DefaultExecutionController {
    fn execute_with_control(&self, plan: &ExecutionPlan) -> ExecutionResult {
        if let Err(error) = validate_execution_plan(plan) {
            return self.validation_failure(plan, error);
        }

        let workspace = match self.environment_manager.prepare_isolated(plan) {
            Ok(workspace) => workspace,
            Err(error) => return self.environment_failure(plan, error),
        };
        let working_dir_hash = match self.filesystem_guard.working_dir_hash(&workspace) {
            Ok(hash) => hash,
            Err(error) => return self.environment_failure(plan, error),
        };
        let snapshot_seed = match self.reproducibility_manager.snapshot(
            &plan.language,
            &workspace.project_root,
            working_dir_hash.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => return self.environment_failure(plan, error),
        };
        let container = match self
            .container_manager
            .create_container(&snapshot_seed, &workspace)
        {
            Ok(container) => container,
            Err(error) => return self.environment_failure(plan, error),
        };

        let mut traces = Vec::new();
        let mut failure_type = None;
        let dependency_result = self.execute_dependency_step(
            &workspace,
            &container,
            &plan.dependency_plan,
            &mut traces,
        );
        if !dependency_result.success {
            failure_type = Some(self.failure_analyzer.classify_step_failure(
                "dependency",
                &dependency_result.stderr,
                dependency_result.stderr.contains("timed out"),
                false,
            ));
        }

        let build_result = if dependency_result.success || !self.stop_on_failure {
            let result = self.execute_phase(
                "build",
                &workspace,
                &container,
                &plan.build_plan.build_commands,
                self.timeout_policy.build_timeout_ms,
                &mut traces,
            );
            if !result.success && failure_type.is_none() {
                failure_type = Some(self.failure_analyzer.classify_step_failure(
                    "build",
                    &result.stderr,
                    result.stderr.contains("timed out"),
                    false,
                ));
            }
            result
        } else {
            StepResult::skipped("build skipped due to dependency failure")
        };

        let run_result =
            if (dependency_result.success && build_result.success) || !self.stop_on_failure {
                let result = self.execute_phase(
                    "run",
                    &workspace,
                    &container,
                    &plan.run_plan.run_commands,
                    self.timeout_policy.run_timeout_ms,
                    &mut traces,
                );
                if !result.success && failure_type.is_none() {
                    failure_type = Some(self.failure_analyzer.classify_step_failure(
                        "run",
                        &result.stderr,
                        result.stderr.contains("timed out"),
                        false,
                    ));
                }
                result
            } else {
                StepResult::skipped("run skipped due to prior failure")
            };

        let test_result =
            if (dependency_result.success && build_result.success && run_result.success)
                || !self.stop_on_failure
            {
                let result = self.execute_phase(
                    "test",
                    &workspace,
                    &container,
                    &plan.test_plan.test_commands,
                    self.timeout_policy.test_timeout_ms,
                    &mut traces,
                );
                if !result.success && failure_type.is_none() {
                    failure_type = Some(self.failure_analyzer.classify_step_failure(
                        "test",
                        &result.stderr,
                        result.stderr.contains("timed out"),
                        false,
                    ));
                }
                result
            } else {
                StepResult::skipped("test skipped due to prior failure")
            };

        let snapshot = match self.reproducibility_manager.snapshot(
            &plan.language,
            &workspace.project_root,
            working_dir_hash,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if failure_type.is_none() {
                    failure_type = Some(FailureType::EnvironmentError);
                }
                ExecutionSnapshot {
                    language: plan.language.clone(),
                    toolchain_version: format!("snapshot-error:{error}"),
                    lockfile_hash: String::new(),
                    os_type: String::new(),
                    architecture: String::new(),
                    env_vars: vec![],
                    working_dir_hash: String::new(),
                }
            }
        };

        let success = dependency_result.success
            && build_result.success
            && run_result.success
            && test_result.success
            && failure_type.is_none();

        let trace = ExecutionTrace {
            execution_id: workspace.execution_id.clone(),
            steps: traces,
        };
        let mut result = ExecutionResult {
            success,
            dependency_result,
            build_result,
            run_result,
            test_result,
            trace,
            failure_type,
            snapshot,
            determinism_report: None,
        };
        if self.config.enable_determinism_check && result.success {
            let replayed = self.replay_engine.replay(&result.snapshot, plan);
            result.determinism_report =
                Some(self.determinism_validator.compare(&result, &replayed));
        }
        let _ = self.container_manager.destroy_container(container);
        let _ = self.environment_manager.cleanup(&workspace);
        result
    }
}

impl DefaultExecutionController {
    fn validation_failure(&self, plan: &ExecutionPlan, error: String) -> ExecutionResult {
        let failed = StepResult {
            success: false,
            stdout: String::new(),
            stderr: error.clone(),
        };
        ExecutionResult {
            success: false,
            dependency_result: failed.clone(),
            build_result: failed.clone(),
            run_result: failed.clone(),
            test_result: failed,
            trace: ExecutionTrace {
                execution_id: "validation-error".to_string(),
                steps: vec![],
            },
            failure_type: Some(FailureType::EnvironmentError),
            snapshot: ExecutionSnapshot {
                language: plan.language.clone(),
                toolchain_version: "unavailable".to_string(),
                lockfile_hash: String::new(),
                os_type: String::new(),
                architecture: String::new(),
                env_vars: vec![],
                working_dir_hash: String::new(),
            },
            determinism_report: None,
        }
    }

    fn environment_failure(&self, plan: &ExecutionPlan, error: String) -> ExecutionResult {
        let failed = StepResult {
            success: false,
            stdout: String::new(),
            stderr: error.clone(),
        };
        ExecutionResult {
            success: false,
            dependency_result: failed.clone(),
            build_result: failed.clone(),
            run_result: failed.clone(),
            test_result: failed,
            trace: ExecutionTrace {
                execution_id: "environment-error".to_string(),
                steps: vec![],
            },
            failure_type: Some(FailureType::EnvironmentError),
            snapshot: ExecutionSnapshot {
                language: plan.language.clone(),
                toolchain_version: format!("environment-error:{error}"),
                lockfile_hash: String::new(),
                os_type: String::new(),
                architecture: String::new(),
                env_vars: vec![],
                working_dir_hash: String::new(),
            },
            determinism_report: None,
        }
    }

    fn execute_dependency_step(
        &self,
        workspace: &Workspace,
        container: &crate::container::container_manager::Container,
        plan: &DependencyPlan,
        traces: &mut Vec<StepTrace>,
    ) -> StepResult {
        if self.dry_run {
            let result = StepResult {
                success: true,
                stdout: "dry-run".to_string(),
                stderr: String::new(),
            };
            traces.push(StepTrace {
                step_name: "dependency".to_string(),
                command: vec!["dry-run".to_string()],
                start_time: now_millis(),
                end_time: now_millis(),
                success: true,
                stdout: result.stdout.clone(),
                stderr: result.stderr.clone(),
            });
            return result;
        }

        if let Err(error) = write_manifest(&workspace.project_root, plan) {
            return StepResult {
                success: false,
                stdout: String::new(),
                stderr: error,
            };
        }
        let result = self.execute_phase(
            "dependency",
            workspace,
            container,
            &plan.install_commands,
            self.timeout_policy.dependency_timeout_ms,
            traces,
        );
        if result.success {
            if let Err(error) = write_lockfile(&workspace.project_root, plan) {
                return StepResult {
                    success: false,
                    stdout: result.stdout,
                    stderr: error,
                };
            }
        }
        result
    }

    fn execute_phase(
        &self,
        phase: &str,
        workspace: &Workspace,
        container: &crate::container::container_manager::Container,
        commands: &[String],
        timeout_ms: u64,
        traces: &mut Vec<StepTrace>,
    ) -> StepResult {
        if commands.is_empty() {
            return StepResult::skipped(format!("{phase} skipped: no commands"));
        }
        if self.dry_run {
            for command in commands {
                traces.push(StepTrace {
                    step_name: phase.to_string(),
                    command: command
                        .split_whitespace()
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>(),
                    start_time: now_millis(),
                    end_time: now_millis(),
                    success: true,
                    stdout: "dry-run".to_string(),
                    stderr: String::new(),
                });
            }
            return StepResult {
                success: true,
                stdout: "dry-run".to_string(),
                stderr: String::new(),
            };
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        for command in commands {
            let mut attempt = 0;
            loop {
                let execution =
                    self.execute_command(phase, workspace, container, command, timeout_ms);
                traces.push(execution.trace);
                stdout.push_str(&execution.result.stdout);
                stderr.push_str(&execution.result.stderr);
                if execution.result.success {
                    break;
                }
                let failure = self.failure_analyzer.classify_step_failure(
                    phase,
                    &execution.result.stderr,
                    execution.timed_out,
                    false,
                );
                if self.retry_policy.should_retry(&failure, attempt) {
                    attempt += 1;
                    continue;
                }
                return StepResult {
                    success: false,
                    stdout,
                    stderr,
                };
            }
        }
        StepResult {
            success: true,
            stdout,
            stderr,
        }
    }

    fn execute_command(
        &self,
        phase: &str,
        workspace: &Workspace,
        container: &crate::container::container_manager::Container,
        raw_command: &str,
        timeout_ms: u64,
    ) -> CommandExecution {
        let start_time = now_millis();
        let parsed = parse_command(raw_command);
        let (command_vec, result, timed_out) = match parsed {
            Ok((program, args)) => {
                let command_vec = std::iter::once(program.clone())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>();
                let guard_result =
                    self.network_guard
                        .validate_command(&command_vec)
                        .and_then(|_| {
                            self.filesystem_guard
                                .validate_command(workspace, &command_vec)
                        });
                let result = match guard_result {
                    Ok(()) => {
                        if self.config.use_container {
                            let step = self
                                .container_manager
                                .execute_in_container(container, &command_vec);
                            (step, false)
                        } else {
                            match run_command_with_timeout(
                                &workspace.project_root,
                                &program,
                                &args,
                                timeout_ms,
                            ) {
                                Ok(result) => result,
                                Err(error) => (
                                    StepResult {
                                        success: false,
                                        stdout: String::new(),
                                        stderr: error,
                                    },
                                    false,
                                ),
                            }
                        }
                    }
                    Err(error) => (
                        StepResult {
                            success: false,
                            stdout: String::new(),
                            stderr: error,
                        },
                        false,
                    ),
                };
                (command_vec, result.0, result.1)
            }
            Err(error) => (
                vec![],
                StepResult {
                    success: false,
                    stdout: String::new(),
                    stderr: error,
                },
                false,
            ),
        };
        let end_time = now_millis();
        CommandExecution {
            trace: StepTrace {
                step_name: phase.to_string(),
                command: command_vec,
                start_time,
                end_time,
                success: result.success,
                stdout: result.stdout.clone(),
                stderr: result.stderr.clone(),
            },
            result,
            timed_out,
        }
    }
}

#[derive(Clone, Debug)]
struct CommandExecution {
    result: StepResult,
    trace: StepTrace,
    timed_out: bool,
}

fn run_command_with_timeout(
    project_root: &Path,
    program: &str,
    args: &[String],
    timeout_ms: u64,
) -> Result<(StepResult, bool), String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    let start = SystemTime::now();
    loop {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .map_err(|error| error.to_string())?;
            return Ok((
                StepResult {
                    success: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                },
                false,
            ));
        }
        let elapsed = start.elapsed().map_err(|error| error.to_string())?;
        if elapsed >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|error| error.to_string())?;
            return Ok((
                StepResult {
                    success: false,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: if output.stderr.is_empty() {
                        "command timed out".to_string()
                    } else {
                        format!(
                            "{}\ncommand timed out",
                            String::from_utf8_lossy(&output.stderr).trim()
                        )
                    },
                },
                true,
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
