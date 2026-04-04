use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::coding::{compute_diff_report, generate_code_change_set, transactional_apply};
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_integration_{name}_{unique}"));
    fs::create_dir_all(dir.join("crates/agent_core/src/engine")).expect("engine dir");
    fs::create_dir_all(dir.join("crates/execution_core/src/dependency")).expect("dependency dir");
    dir
}

#[test]
fn workspace_symbol_rebinding_rewrites_to_public_cross_crate_path() {
    let workspace = temp_workspace("workspace_symbol_rebinding");
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/agent_core\", \"crates/execution_core\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        workspace.join("crates/agent_core/Cargo.toml"),
        "[package]\nname = \"agent_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nexecution_core = { path = \"../execution_core\" }\n",
    )
    .expect("agent cargo");
    fs::write(
        workspace.join("crates/execution_core/Cargo.toml"),
        "[package]\nname = \"execution_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("execution cargo");
    fs::write(
        workspace.join("crates/agent_core/src/lib.rs"),
        "pub mod engine;\n",
    )
    .expect("agent lib");
    fs::write(
        workspace.join("crates/agent_core/src/engine/mod.rs"),
        "pub fn run() {}\n",
    )
    .expect("engine");
    fs::write(
        workspace.join("crates/execution_core/src/lib.rs"),
        "pub mod dependency;\n",
    )
    .expect("execution lib");
    fs::write(
        workspace.join("crates/execution_core/src/dependency/mod.rs"),
        "pub fn noop() {}\n",
    )
    .expect("dependency mod");

    let patches = vec![
        CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("dependency".to_string(), "engine".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "DependencyEngineInterface".to_string(),
                between: ("dependency".to_string(), "engine".to_string()),
            }],
            description: "create cross crate interface".to_string(),
        },
        CodePatch {
            patch_id: "p2".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "engine".to_string(),
                to: "dependency".to_string(),
                via: Some("DependencyEngineInterface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "engine".to_string(),
                to: "dependency".to_string(),
                via: Some("DependencyEngineInterface".to_string()),
            }],
            description: "rebind engine import".to_string(),
        },
    ];

    let change_set = generate_code_change_set(&workspace, &patches).expect("change set");
    let diff = compute_diff_report(&workspace, &change_set).expect("diff");
    let result = transactional_apply(&workspace, &change_set, None, false).expect("apply");

    assert!(result.applied, "{:?}", result.diagnostics);
    assert!(result.build_ok, "{:?}", result.diagnostics);

    let engine_mod =
        fs::read_to_string(workspace.join("crates/agent_core/src/engine/mod.rs")).expect("engine");
    assert!(
        engine_mod.contains(
            "use execution_core::dependency::dependency_engine_interface::DependencyEngineInterface;"
        ),
        "{engine_mod}"
    );
    let dependency_mod =
        fs::read_to_string(workspace.join("crates/execution_core/src/dependency/mod.rs"))
            .expect("dependency mod");
    assert!(
        dependency_mod.contains("pub mod dependency_engine_interface;"),
        "{dependency_mod}"
    );
    assert!(
        diff.diffs.iter().any(|entry| {
            entry.target.contains(
                "ImportRebinding: use execution_core::dependency::dependency_engine_interface::DependencyEngineInterface;"
            )
        }),
        "{:?}",
        diff.diffs
    );
}
