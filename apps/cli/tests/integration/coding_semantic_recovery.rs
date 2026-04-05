use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_semantic_recovery_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/agent")).expect("agent dir");
    dir
}

fn write_empty_patches(workspace: &std::path::Path) -> PathBuf {
    let path = workspace.join("patches.json");
    fs::write(&path, "{ \"patches\": [] }\n").expect("patches");
    path
}

#[test]
fn coding_semantic_recovery_uses_rustc_help_for_missing_import() {
    let workspace = temp_workspace("help_import");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"semantic_recovery_help\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod domain;\nmod agent;\nfn main() {}\n",
    )
    .expect("main");
    fs::write(
        workspace.join("src/domain.rs"),
        "pub struct AgentInput;\npub struct AgentOutput;\npub enum DomainError {}\n",
    )
    .expect("domain");
    fs::write(
        workspace.join("src/agent/mod.rs"),
        "pub trait Agent {\n    fn execute(input: AgentInput);\n}\n",
    )
    .expect("agent");
    let patch_path = write_empty_patches(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_design_cli"))
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--check",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\": \"checked\""), "{stdout}");
    assert!(stdout.contains("\"build_ok\": true"), "{stdout}");

    let telemetry = fs::read_to_string(workspace.join(".dbm/telemetry/semantic_recovery.json"))
        .expect("semantic recovery telemetry");
    assert!(
        telemetry.contains("\"error_type\": \"MissingType\""),
        "{telemetry}"
    );
    assert!(
        telemetry.contains("\"used_rustc_help\": true"),
        "{telemetry}"
    );
    assert!(
        telemetry.contains("\"patch_family\": \"safe_import_fix\""),
        "{telemetry}"
    );

    let original = fs::read_to_string(workspace.join("src/agent/mod.rs")).expect("agent");
    assert!(
        !original.contains("use crate::domain::AgentInput;"),
        "{original}"
    );
}

#[test]
fn coding_semantic_recovery_preserves_green_import_hub() {
    let workspace = temp_workspace("green_hub");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"semantic_recovery_green_hub\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod domain;\nmod agent;\nfn main() {}\n",
    )
    .expect("main");
    fs::write(
        workspace.join("src/domain.rs"),
        "pub struct AgentInput;\npub struct AgentOutput;\npub enum DomainError {}\n",
    )
    .expect("domain");
    fs::write(
        workspace.join("src/agent/mod.rs"),
        "use crate::agent_capability_interface;\n\npub use crate::domain::{AgentInput, AgentOutput, DomainError};\n\npub trait Agent {\n    fn execute(input: AgentInput) -> Result<AgentOutput, DomainError>;\n}\n",
    )
    .expect("agent");
    let patch_path = write_empty_patches(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_design_cli"))
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--check",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\": \"checked\""), "{stdout}");
    assert!(stdout.contains("\"build_ok\": true"), "{stdout}");

    let telemetry = fs::read_to_string(workspace.join(".dbm/telemetry/semantic_recovery.json"))
        .expect("semantic recovery telemetry");
    assert!(
        telemetry.contains("\"error_type\": \"MissingImport\""),
        "{telemetry}"
    );
    assert!(
        telemetry.contains("\"green_state_preserved\": true"),
        "{telemetry}"
    );

    let original = fs::read_to_string(workspace.join("src/agent/mod.rs")).expect("agent");
    assert!(
        original.contains("use crate::agent_capability_interface;"),
        "{original}"
    );
    assert!(
        original.contains("pub use crate::domain::{AgentInput, AgentOutput, DomainError};"),
        "{original}"
    );
}
