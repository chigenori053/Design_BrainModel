use std::{fs, path::PathBuf};

use design_cli::mutation_integration::{MutationEngineDispatcher, predict_mutation_preview};
use mutation_engine::{
    AnalyzeContext, DependencyEdge, FileDiff, MutationEngine, MutationOperation, MutationPlan,
    MutationPreview, MutationTarget, NoopRuntimeValidator, PatchOperation,
};
use tempfile::tempdir;
use world_model::{MutationValidationGate, ValidationDecision};

#[test]
fn plan_preview_predict_validate_flow_is_deterministic() {
    let context = AnalyzeContext {
        nodes: vec!["runtime".to_string(), "policy".to_string()],
        dependencies: vec![DependencyEdge {
            from: "runtime".to_string(),
            to: "policy".to_string(),
        }],
        ..AnalyzeContext::default()
    };
    let plan = MutationPlan {
        id: "mutation-flow".to_string(),
        target: MutationTarget::File(PathBuf::from("src/lib.rs")),
        operation: MutationOperation::Refactor,
        reason: "test".to_string(),
        expected_effect: "test".to_string(),
        patches: vec![PatchOperation::Write {
            path: PathBuf::from("src/lib.rs"),
            content: "pub fn updated() {}".to_string(),
        }],
        design_intent: Vec::new(),
        projected_dependencies: None,
    };
    let preview = MutationPreview {
        mutation_id: plan.id.clone(),
        files: vec![FileDiff {
            path: PathBuf::from("src/lib.rs"),
            unified_diff: "+pub fn updated() {}".to_string(),
        }],
        confirmation_required: false,
    };

    let left = predict_mutation_preview(&context, &plan, &preview);
    let right = predict_mutation_preview(&context, &plan, &preview);

    assert_eq!(left, right);
    assert_ne!(
        MutationValidationGate::decide(&left),
        ValidationDecision::Reject
    );
}

#[test]
fn stable_prediction_allows_apply() {
    let workspace = mutation_workspace("pub fn before() {}\n");
    let plan = plan_with_dependencies(
        "stable-apply",
        "pub fn after() {}\n",
        vec![DependencyEdge {
            from: "runtime".to_string(),
            to: "policy".to_string(),
        }],
    );
    persist_plan(workspace.path(), plan);

    let result = MutationEngineDispatcher::apply(workspace.path(), "stable-apply");

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap(),
        "pub fn after() {}\n"
    );
}

#[test]
fn contradictory_prediction_rejects_apply_without_changing_files() {
    let workspace = mutation_workspace("pub fn before() {}\n");
    let plan = plan_with_dependencies(
        "contradictory-apply",
        "pub fn after() {}\n",
        vec![
            DependencyEdge {
                from: "runtime".to_string(),
                to: "policy".to_string(),
            },
            DependencyEdge {
                from: "runtime".to_string(),
                to: "memory".to_string(),
            },
        ],
    );
    persist_plan(workspace.path(), plan);

    let error = MutationEngineDispatcher::apply(workspace.path(), "contradictory-apply")
        .expect_err("contradictory prediction must reject apply");

    assert!(error.contains("causal validation gate"), "{error}");
    assert!(error.contains("主な原因"), "{error}");
    assert!(error.contains("影響が予測される領域"), "{error}");
    assert!(error.contains("推奨事項"), "{error}");
    assert_eq!(
        fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap(),
        "pub fn before() {}\n"
    );
}

fn mutation_workspace(content: &str) -> tempfile::TempDir {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("src/lib.rs"), content).unwrap();
    workspace
}

fn plan_with_dependencies(
    id: &str,
    content: &str,
    projected_dependencies: Vec<DependencyEdge>,
) -> MutationPlan {
    MutationPlan {
        id: id.to_string(),
        target: MutationTarget::File(PathBuf::from("src/lib.rs")),
        operation: MutationOperation::Update,
        reason: "validate future structure".to_string(),
        expected_effect: "preserve structural stability".to_string(),
        patches: vec![PatchOperation::Write {
            path: PathBuf::from("src/lib.rs"),
            content: content.to_string(),
        }],
        design_intent: Vec::new(),
        projected_dependencies: Some(projected_dependencies),
    }
}

fn persist_plan(workspace: &std::path::Path, plan: MutationPlan) {
    let runtime = NoopRuntimeValidator;
    MutationEngine::new(workspace, "test", &runtime)
        .plan(plan)
        .unwrap();
}
