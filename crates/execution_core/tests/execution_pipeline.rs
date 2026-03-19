use execution_core::engine::execution_engine::{DefaultExecutionEngine, ExecutionEngine};
use execution_core::engine::execution_plan::{
    BuildPlan, DependencyPlan, DependencySpec, ExecutionPlan, RunPlan, TargetLanguage, TestPlan,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project_root(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_brain_model_{name}_{id}"));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

#[test]
fn dependency_resolution_test() {
    let root = temp_project_root("deps");
    let plan = ExecutionPlan {
        language: TargetLanguage::Rust,
        framework: None,
        project_root: root.clone(),
        dependency_plan: DependencyPlan {
            manifest_file: "manifest.txt".into(),
            dependencies: vec![DependencySpec {
                name: "serde".into(),
                version: Some("1".into()),
            }],
            install_commands: vec!["rustc --version".into()],
        },
        build_plan: BuildPlan::default(),
        run_plan: RunPlan::default(),
        test_plan: TestPlan::default(),
    };

    let engine = DefaultExecutionEngine::default();
    let result = engine.execute(&plan);

    assert!(result.dependency_result.success);
    assert!(root.join("manifest.txt").exists());
    assert!(root.join("execution.lock").exists());
}

#[test]
fn build_run_test_execute() {
    let root = temp_project_root("build_run");
    let plan = ExecutionPlan {
        language: TargetLanguage::Rust,
        framework: None,
        project_root: root,
        dependency_plan: DependencyPlan {
            manifest_file: "manifest.txt".into(),
            dependencies: vec![],
            install_commands: vec!["rustc --version".into()],
        },
        build_plan: BuildPlan {
            build_commands: vec!["rustc --version".into()],
        },
        run_plan: RunPlan {
            run_commands: vec!["rustc --version".into()],
        },
        test_plan: TestPlan {
            test_files: vec![],
            test_commands: vec!["rustc --version".into()],
        },
    };

    let result = DefaultExecutionEngine::default().execute(&plan);
    assert!(result.success);
    assert!(result.build_result.success);
    assert!(result.run_result.success);
    assert!(result.test_result.success);
}

#[test]
fn failure_handling_test_invalid_dependency_command() {
    let root = temp_project_root("failure");
    let plan = ExecutionPlan {
        language: TargetLanguage::Rust,
        framework: None,
        project_root: root,
        dependency_plan: DependencyPlan {
            manifest_file: "manifest.txt".into(),
            dependencies: vec![],
            install_commands: vec!["command_that_does_not_exist".into()],
        },
        build_plan: BuildPlan {
            build_commands: vec!["rustc --version".into()],
        },
        run_plan: RunPlan {
            run_commands: vec!["rustc --version".into()],
        },
        test_plan: TestPlan {
            test_files: vec![],
            test_commands: vec!["rustc --version".into()],
        },
    };

    let result = DefaultExecutionEngine::default().execute(&plan);
    assert!(!result.success);
    assert!(!result.dependency_result.success);
}
