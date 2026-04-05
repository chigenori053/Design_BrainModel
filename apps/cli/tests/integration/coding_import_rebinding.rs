use std::fs;
use std::path::PathBuf;
use std::process::Command;
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

#[test]
fn coding_check_preserves_domain_reexport_imports() {
    let workspace = temp_workspace("agent_core_import_guard");
    fs::create_dir_all(workspace.join("crates/agent_core/src/agent")).expect("agent dir");
    fs::create_dir_all(workspace.join("crates/agent_core/src/domain")).expect("domain dir");
    fs::create_dir_all(workspace.join("crates/agent_core/src/ports")).expect("ports dir");
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/agent_core\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        workspace.join("crates/agent_core/Cargo.toml"),
        "[package]\nname = \"agent_core\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
    )
    .expect("agent_core cargo");
    fs::write(
        workspace.join("crates/agent_core/src/lib.rs"),
        "pub mod agent;\npub mod domain;\npub mod ports;\n",
    )
    .expect("lib");
    fs::write(
        workspace.join("crates/agent_core/src/domain/mod.rs"),
        "pub struct AgentInput;\npub struct AgentOutput;\npub enum DomainError {}\n",
    )
    .expect("domain");
    fs::write(
        workspace.join("crates/agent_core/src/ports/mod.rs"),
        "pub trait MemoryPort {}\npub trait TelemetryPort {}\n",
    )
    .expect("ports");
    fs::write(
        workspace.join("crates/agent_core/src/agent/design_agent.rs"),
        "pub struct DesignAgent;\n",
    )
    .expect("design_agent");
    fs::write(
        workspace.join("crates/agent_core/src/agent/learning_agent.rs"),
        "pub struct LearningAgent;\n",
    )
    .expect("learning_agent");
    fs::write(
        workspace.join("crates/agent_core/src/agent/search_agent.rs"),
        "pub struct SearchAgent;\n",
    )
    .expect("search_agent");
    fs::write(
        workspace.join("crates/agent_core/src/agent/websearch_agent.rs"),
        "pub struct WebSearchAgent;\n",
    )
    .expect("websearch_agent");
    fs::write(
        workspace.join("crates/agent_core/src/agent/mod.rs"),
        "pub mod design_agent;\npub mod learning_agent;\npub mod search_agent;\npub mod websearch_agent;\n\nuse crate::domain::{AgentInput, AgentOutput, DomainError};\nuse crate::ports::{MemoryPort, TelemetryPort};\n\npub struct AgentContext<'a> {\n    pub memory: &'a dyn MemoryPort,\n    pub telemetry: &'a dyn TelemetryPort,\n}\n\npub trait Agent: Send {\n    fn name(&self) -> &'static str;\n\n    fn handle(\n        &mut self,\n        input: AgentInput,\n        ctx: &AgentContext<'_>,\n    ) -> Result<AgentOutput, DomainError>;\n}\n\npub use design_agent::DesignAgent;\npub use learning_agent::LearningAgent;\npub use search_agent::SearchAgent;\npub use websearch_agent::WebSearchAgent;\n",
    )
    .expect("agent mod");
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            ".",
            "--target",
            "crates/agent_core/src/agent/mod.rs",
            "--check",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("use crate::agent_capability_interface;"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("use crate::agent_domain_interface;"),
        "{stdout}"
    );
    assert!(!stdout.contains("ModRegistration:"), "{stdout}");
    assert!(
        stdout.contains("Patches (canonical): 0")
            || stdout.contains("Patches (canonical): 1")
            || stdout.contains("Patches (canonical): 2"),
        "{stdout}"
    );
}
