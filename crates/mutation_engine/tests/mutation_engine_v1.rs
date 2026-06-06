use std::fs;
use std::path::PathBuf;
use std::{cell::Cell, path::Path};

use mutation_engine::{
    AnalyzeContext, Confirmation, DependencyEdge, MutationAuditLog, MutationEngine, MutationError,
    MutationOperation, MutationPlanner, MutationReplayEngine, MutationRequest,
    MutationRollbackEngine, MutationTarget, NoopRuntimeValidator, PatchOperation,
    RuntimeCheckResult, RuntimeValidator,
};
use tempfile::tempdir;

fn request(operation: MutationOperation, patches: Vec<PatchOperation>) -> MutationRequest {
    MutationRequest {
        target: MutationTarget::File(PathBuf::from("src/lib.rs")),
        operation,
        reason: "align runtime structure".to_string(),
        expected_effect: "preserve the boundary".to_string(),
        patches,
        projected_dependencies: None,
    }
}

#[test]
fn enforces_validate_preview_apply_and_persists_audit() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("src/lib.rs"), "pub fn old() {}\n").unwrap();
    let analyze = AnalyzeContext {
        design_intent: vec!["preserve core boundary".to_string()],
        ..AnalyzeContext::default()
    };
    let plan = MutationPlanner.plan(
        &analyze,
        request(
            MutationOperation::Update,
            vec![PatchOperation::Write {
                path: PathBuf::from("src/lib.rs"),
                content: "pub fn updated() {}\n".to_string(),
            }],
        ),
    );
    let id = plan.id.clone();
    let runtime = NoopRuntimeValidator;
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    let previewed = engine
        .plan(plan)
        .unwrap()
        .validate(&analyze)
        .unwrap()
        .preview()
        .unwrap();
    assert!(
        previewed.preview_data().files[0]
            .unified_diff
            .contains("+pub fn updated() {}")
    );
    previewed.apply(None).unwrap();

    assert_eq!(
        fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap(),
        "pub fn updated() {}\n"
    );
    let audit = workspace
        .path()
        .join(".dbm/mutations")
        .join(id)
        .join("mutation_audit.json");
    assert!(audit.exists());
}

#[test]
fn dangerous_mutation_requires_confirmation_and_can_rollback_and_replay() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("src/obsolete.rs"), "old\n").unwrap();
    let analyze = AnalyzeContext::default();
    let plan = MutationPlanner.plan(
        &analyze,
        request(
            MutationOperation::Delete,
            vec![PatchOperation::Delete {
                path: PathBuf::from("src/obsolete.rs"),
            }],
        ),
    );
    let id = plan.id.clone();
    let runtime = NoopRuntimeValidator;
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    let previewed = engine
        .plan(plan)
        .unwrap()
        .validate(&analyze)
        .unwrap()
        .preview()
        .unwrap();
    assert!(matches!(
        previewed.apply(None),
        Err(MutationError::ConfirmationRequired)
    ));

    let plan = MutationPlanner.plan(
        &analyze,
        request(
            MutationOperation::Delete,
            vec![PatchOperation::Delete {
                path: PathBuf::from("src/obsolete.rs"),
            }],
        ),
    );
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    engine
        .plan(plan)
        .unwrap()
        .validate(&analyze)
        .unwrap()
        .preview()
        .unwrap()
        .apply(Some(&Confirmation {
            mutation_id: id.clone(),
            approved_by: "reviewer".to_string(),
        }))
        .unwrap();
    assert!(!workspace.path().join("src/obsolete.rs").exists());

    MutationRollbackEngine::new(workspace.path())
        .rollback(&id, "test")
        .unwrap();
    assert_eq!(
        fs::read_to_string(workspace.path().join("src/obsolete.rs")).unwrap(),
        "old\n"
    );
    MutationReplayEngine::new(workspace.path())
        .replay(&id, "test")
        .unwrap();
    assert!(!workspace.path().join("src/obsolete.rs").exists());
    let audit: MutationAuditLog = serde_json::from_slice(
        &fs::read(
            workspace
                .path()
                .join(".dbm/mutations")
                .join(id)
                .join("mutation_audit.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(audit.events.len(), 3);
}

#[test]
fn rejects_paths_outside_workspace_and_forbidden_commands() {
    let workspace = tempdir().unwrap();
    let analyze = AnalyzeContext::default();
    let plan = MutationPlanner.plan(
        &analyze,
        request(
            MutationOperation::Update,
            vec![PatchOperation::Write {
                path: PathBuf::from("../escape.sh"),
                content: "rm -rf /".to_string(),
            }],
        ),
    );
    let runtime = NoopRuntimeValidator;
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    let result = engine.plan(plan).unwrap().validate(&analyze);
    assert!(matches!(result, Err(MutationError::Rejected(_))));
}

#[test]
fn validates_the_projected_graph_when_removing_a_cycle() {
    let workspace = tempdir().unwrap();
    let analyze = AnalyzeContext {
        nodes: vec!["a".to_string(), "b".to_string()],
        dependencies: vec![
            DependencyEdge {
                from: "a".to_string(),
                to: "b".to_string(),
            },
            DependencyEdge {
                from: "b".to_string(),
                to: "a".to_string(),
            },
        ],
        ..AnalyzeContext::default()
    };
    let mut request = request(
        MutationOperation::Refactor,
        vec![PatchOperation::Write {
            path: PathBuf::from("src/lib.rs"),
            content: "pub trait Boundary {}\n".to_string(),
        }],
    );
    request.projected_dependencies = Some(vec![DependencyEdge {
        from: "a".to_string(),
        to: "b".to_string(),
    }]);
    let plan = MutationPlanner.plan(&analyze, request);
    let runtime = NoopRuntimeValidator;
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    let validated = engine.plan(plan).unwrap().validate(&analyze);
    assert!(validated.is_ok());
}

struct FailAfterValidation {
    calls: Cell<usize>,
}

impl RuntimeValidator for FailAfterValidation {
    fn validate(
        &self,
        _workspace: &Path,
        _plan: &mutation_engine::MutationPlan,
    ) -> Vec<RuntimeCheckResult> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        vec![RuntimeCheckResult {
            name: "cargo check".to_string(),
            passed: call == 0,
            detail: if call == 0 {
                "passed".to_string()
            } else {
                "failed".to_string()
            },
        }]
    }
}

#[test]
fn failed_post_apply_verification_restores_the_workspace() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("src/lib.rs"), "before\n").unwrap();
    let analyze = AnalyzeContext::default();
    let plan = MutationPlanner.plan(
        &analyze,
        request(
            MutationOperation::Update,
            vec![PatchOperation::Write {
                path: PathBuf::from("src/lib.rs"),
                content: "after\n".to_string(),
            }],
        ),
    );
    let runtime = FailAfterValidation {
        calls: Cell::new(0),
    };
    let engine = MutationEngine::new(workspace.path(), "test", &runtime);
    let result = engine
        .plan(plan)
        .unwrap()
        .validate(&analyze)
        .unwrap()
        .preview()
        .unwrap()
        .apply(None);

    assert!(matches!(result, Err(MutationError::VerificationFailed(_))));
    assert_eq!(
        fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap(),
        "before\n"
    );
}
