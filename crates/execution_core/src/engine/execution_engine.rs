use crate::dependency::resolver::{DefaultDependencyResolver, DependencyResolver};
use crate::engine::execution_plan::ExecutionPlan;
use crate::engine::execution_result::{ExecutionResult, StepResult};
use crate::environment::env_manager::{EnvironmentManager, LocalEnvironmentManager};
use crate::executor::build_executor::{BuildExecutor, DefaultBuildExecutor};
use crate::executor::run_executor::{DefaultRunExecutor, RunExecutor};
use crate::executor::test_executor::{DefaultTestExecutor, TestExecutor};
use crate::validation::validate_execution_plan;

pub trait ExecutionEngine {
    fn execute(&self, plan: &ExecutionPlan) -> ExecutionResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultExecutionEngine {
    pub dependency_resolver: DefaultDependencyResolver,
    pub build_executor: DefaultBuildExecutor,
    pub run_executor: DefaultRunExecutor,
    pub test_executor: DefaultTestExecutor,
    pub environment_manager: LocalEnvironmentManager,
}

impl ExecutionEngine for DefaultExecutionEngine {
    fn execute(&self, plan: &ExecutionPlan) -> ExecutionResult {
        if let Err(error) = validate_execution_plan(plan) {
            let failed = StepResult {
                success: false,
                stdout: String::new(),
                stderr: error,
            };
            return ExecutionResult {
                success: false,
                dependency_result: failed.clone(),
                build_result: failed.clone(),
                run_result: failed.clone(),
                test_result: failed,
                logs: vec!["validation failed".into()],
            };
        }

        let mut logs = Vec::new();
        let prepared = self.environment_manager.prepare(plan);
        if let Err(error) = prepared {
            let failed = StepResult {
                success: false,
                stdout: String::new(),
                stderr: error,
            };
            return ExecutionResult {
                success: false,
                dependency_result: failed.clone(),
                build_result: failed.clone(),
                run_result: failed.clone(),
                test_result: failed,
                logs,
            };
        }

        logs.push(format!("project_root={}", plan.project_root.display()));

        let dependency_result = self
            .dependency_resolver
            .resolve(&plan.project_root, &plan.dependency_plan);
        logs.push("dependency resolution executed".into());

        let build_result = if dependency_result.success {
            self.build_executor
                .build(&plan.project_root, &plan.build_plan)
        } else {
            StepResult::skipped("build skipped due to dependency failure")
        };
        logs.push("build executed".into());

        let run_result = if dependency_result.success && build_result.success {
            self.run_executor.run(&plan.project_root, &plan.run_plan)
        } else {
            StepResult::skipped("run skipped due to prior failure")
        };
        logs.push("run executed".into());

        let test_result = if dependency_result.success && build_result.success {
            self.test_executor.test(&plan.project_root, &plan.test_plan)
        } else {
            StepResult::skipped("test skipped due to prior failure")
        };
        logs.push("test executed".into());

        let success = dependency_result.success
            && build_result.success
            && run_result.success
            && test_result.success;

        let _ = self.environment_manager.cleanup();

        ExecutionResult {
            success,
            dependency_result,
            build_result,
            run_result,
            test_result,
            logs,
        }
    }
}
