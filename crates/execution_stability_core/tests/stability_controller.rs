use execution_core::engine::execution_plan::{
    BuildPlan, DependencyPlan, DependencySpec, ExecutionPlan, RunPlan, TargetLanguage, TestPlan,
};
use execution_stability_core::stable_v03::{
    DefaultExecutionController, EnvironmentManager, ExecutionConfig, ExecutionController,
    FailureType, IsolatedEnvironmentManager, NetworkMode, ReplayEngine, TimeoutPolicy,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project_root(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("dbm_phase7_{name}_{id}"));
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("execution.lock"), "serde:1\n").expect("write lock");
    root
}

fn rust_plan(root: PathBuf) -> ExecutionPlan {
    ExecutionPlan {
        language: TargetLanguage::Rust,
        framework: None,
        project_root: root,
        dependency_plan: DependencyPlan {
            manifest_file: "manifest.txt".to_string(),
            dependencies: vec![DependencySpec {
                name: "serde".to_string(),
                version: Some("1".to_string()),
            }],
            install_commands: vec!["rustc --version".to_string()],
        },
        build_plan: BuildPlan {
            build_commands: vec!["rustc --version".to_string()],
        },
        run_plan: RunPlan {
            run_commands: vec!["rustc --version".to_string()],
        },
        test_plan: TestPlan {
            test_files: vec![],
            test_commands: vec!["rustc --version".to_string()],
        },
    }
}

fn deterministic_controller() -> DefaultExecutionController {
    let mut controller = DefaultExecutionController {
        config: ExecutionConfig {
            use_container: false,
            network_mode: NetworkMode::Disabled,
            enable_determinism_check: false,
        },
        ..Default::default()
    };
    controller.network_guard.mode = controller.config.network_mode.clone();
    controller
}

fn normalize_trace(
    result: &execution_stability_core::stable_v03::ExecutionResult,
) -> Vec<(String, Vec<String>, bool, String, String)> {
    result
        .trace
        .steps
        .iter()
        .map(|step| {
            (
                step.step_name.clone(),
                step.command.clone(),
                step.success,
                step.stdout.clone(),
                step.stderr.clone(),
            )
        })
        .collect()
}

#[test]
fn isolation_manager_creates_distinct_workspaces() {
    let root = temp_project_root("isolation");
    fs::write(root.join("main.txt"), "source").expect("seed source");
    let plan = rust_plan(root.clone());
    let manager = IsolatedEnvironmentManager {
        cleanup_on_drop: false,
        ..IsolatedEnvironmentManager::default()
    };

    let first = manager.prepare_isolated(&plan).expect("first workspace");
    let second = manager.prepare_isolated(&plan).expect("second workspace");

    assert_ne!(first.root_dir, second.root_dir);
    assert!(first.project_root.join("main.txt").exists());
    assert!(second.project_root.join("main.txt").exists());

    manager.cleanup(&first).expect("cleanup first");
    manager.cleanup(&second).expect("cleanup second");
}

#[test]
fn same_plan_produces_same_trace_shape_except_timestamps() {
    let root = temp_project_root("determinism");
    let plan = rust_plan(root.clone());
    let controller = deterministic_controller();

    let lhs = controller.execute_with_control(&plan);
    let rhs = controller.execute_with_control(&plan);

    assert!(lhs.success);
    assert!(rhs.success);
    assert_eq!(normalize_trace(&lhs), normalize_trace(&rhs));
}

#[test]
fn timeout_is_classified_deterministically() {
    let root = temp_project_root("timeout");
    let mut plan = rust_plan(root);
    plan.run_plan.run_commands = vec!["sleep 1".to_string()];
    let mut controller = deterministic_controller();
    controller.timeout_policy = TimeoutPolicy {
        run_timeout_ms: 20,
        ..TimeoutPolicy::default()
    };

    let result = controller.execute_with_control(&plan);

    assert!(!result.success);
    assert_eq!(result.failure_type, Some(FailureType::Timeout));
    assert!(!result.run_result.success);
}

#[test]
fn invalid_dependency_is_classified_as_dependency_failure() {
    let root = temp_project_root("dependency_failure");
    let mut plan = rust_plan(root);
    plan.dependency_plan.install_commands = vec!["command_that_does_not_exist".to_string()];

    let result = deterministic_controller().execute_with_control(&plan);

    assert!(!result.success);
    assert_eq!(result.failure_type, Some(FailureType::DependencyFailure));
}

#[test]
fn dry_run_skips_command_execution() {
    let root = temp_project_root("dry_run");
    let mut plan = rust_plan(root);
    plan.run_plan.run_commands = vec!["sleep 1".to_string()];
    let mut controller = deterministic_controller();
    controller.dry_run = true;

    let result = controller.execute_with_control(&plan);

    assert!(result.success);
    assert!(
        result
            .trace
            .steps
            .iter()
            .all(|step| step.stdout == "dry-run")
    );
    assert_eq!(result.failure_type, None);
}

#[test]
fn identical_environment_produces_identical_snapshot() {
    let root = temp_project_root("snapshot");
    let plan = rust_plan(root);
    let controller = deterministic_controller();

    let lhs = controller.execute_with_control(&plan);
    let rhs = controller.execute_with_control(&plan);

    assert_eq!(lhs.snapshot, rhs.snapshot);
    assert!(!lhs.snapshot.os_type.is_empty());
    assert!(!lhs.snapshot.architecture.is_empty());
    assert!(!lhs.snapshot.working_dir_hash.is_empty());
}

#[test]
fn container_fallback_execution_matches_local_behavior() {
    let root = temp_project_root("container");
    let plan = rust_plan(root);
    let mut controller = deterministic_controller();
    controller.config.use_container = true;

    let result = controller.execute_with_control(&plan);

    assert!(result.success);
    assert!(
        result
            .trace
            .steps
            .iter()
            .all(|step| !step.command.is_empty())
    );
}

#[test]
fn replay_engine_reproduces_same_observable_result() {
    let root = temp_project_root("replay");
    let plan = rust_plan(root);
    let controller = deterministic_controller();

    let first = controller.execute_with_control(&plan);
    let replayed = controller.replay_engine.replay(&first.snapshot, &plan);

    assert!(replayed.success);
    assert_eq!(normalize_trace(&first), normalize_trace(&replayed));
    assert_eq!(first.snapshot, replayed.snapshot);
}

#[test]
fn filesystem_restriction_blocks_external_writes() {
    let root = temp_project_root("fs_guard");
    let mut plan = rust_plan(root);
    plan.run_plan.run_commands = vec!["touch /tmp/dbm_phase75_outside".to_string()];

    let result = deterministic_controller().execute_with_control(&plan);

    assert!(!result.success);
    assert_eq!(result.failure_type, Some(FailureType::RuntimeFailure));
    assert!(
        result
            .run_result
            .stderr
            .contains("filesystem guard blocked")
    );
}

#[test]
fn network_blocking_prevents_external_access() {
    let root = temp_project_root("network_guard");
    let mut plan = rust_plan(root);
    plan.run_plan.run_commands = vec!["curl https://example.com".to_string()];

    let result = deterministic_controller().execute_with_control(&plan);

    assert!(!result.success);
    assert_eq!(result.failure_type, Some(FailureType::RuntimeFailure));
    assert!(result.run_result.stderr.contains("network guard blocked"));
}

#[test]
fn determinism_check_produces_report() {
    let root = temp_project_root("determinism_report");
    let plan = rust_plan(root);
    let mut controller = deterministic_controller();
    controller.config.enable_determinism_check = true;

    let result = controller.execute_with_control(&plan);

    assert!(result.success);
    assert_eq!(
        result
            .determinism_report
            .as_ref()
            .map(|report| report.is_deterministic),
        Some(true)
    );
}
