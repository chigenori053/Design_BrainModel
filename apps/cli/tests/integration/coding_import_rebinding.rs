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
    fs::create_dir_all(dir.join("crates/execution_stability_core/src/controller"))
        .expect("controller dir");
    dir
}

#[test]
fn coding_import_rebinding_updates_nested_module_tree_and_passes_check() {
    let workspace = temp_workspace("import_rebinding");
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/execution_stability_core\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        workspace.join("crates/execution_stability_core/Cargo.toml"),
        "[package]\nname = \"execution_stability_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("crate cargo");
    fs::write(
        workspace.join("crates/execution_stability_core/src/lib.rs"),
        "pub mod controller;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        workspace.join("crates/execution_stability_core/src/world.rs"),
        "pub fn noop() {}\n",
    )
    .expect("world");
    fs::write(
        workspace.join("crates/execution_stability_core/src/controller/mod.rs"),
        "pub fn check() {}\n",
    )
    .expect("controller mod");

    let patches = vec![
        CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("controller".to_string(), "world".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "ControllerDeterminismInterface".to_string(),
                between: ("controller".to_string(), "world".to_string()),
            }],
            description: "create interface".to_string(),
        },
        CodePatch {
            patch_id: "p2".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "controller".to_string(),
                to: "world".to_string(),
                via: Some("ControllerDeterminismInterface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "controller".to_string(),
                to: "world".to_string(),
                via: Some("ControllerDeterminismInterface".to_string()),
            }],
            description: "rebind import".to_string(),
        },
    ];

    let change_set = generate_code_change_set(&workspace, &patches).expect("change set");
    let diff = compute_diff_report(&workspace, &change_set).expect("diff");
    let result = transactional_apply(&workspace, &change_set, None, false).expect("apply");

    assert!(result.applied, "{:?}", result.diagnostics);
    assert!(result.build_ok, "{:?}", result.diagnostics);

    let controller_mod =
        fs::read_to_string(workspace.join("crates/execution_stability_core/src/controller/mod.rs"))
            .expect("controller mod");
    assert!(
        controller_mod.contains("pub mod controller_determinism_interface;"),
        "{controller_mod}"
    );
    assert!(
        controller_mod.contains(
            "use crate::controller::controller_determinism_interface::ControllerDeterminismInterface;"
        ),
        "{controller_mod}"
    );
    assert!(
        workspace
            .join(
                "crates/execution_stability_core/src/controller/controller_determinism_interface.rs"
            )
            .exists()
    );
    assert!(
        diff.diffs.iter().any(|entry| {
            entry.target.contains(
                "ImportRebinding: use crate::controller::controller_determinism_interface::ControllerDeterminismInterface;"
            )
        }),
        "{:?}",
        diff.diffs
    );
    assert!(
        diff.diffs
            .iter()
            .any(|entry| entry.target.contains("ModRegistration:")),
        "{:?}",
        diff.diffs
    );
}
