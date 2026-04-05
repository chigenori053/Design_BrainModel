use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("design_cli_malformed_import_batch_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("src");
    dir
}

fn write_workspace_manifest(root: &std::path::Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"malformed_import_batch\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/main.rs"),
        "mod coding;\nmod renderer;\nmod app;\nfn main() { let _ = app::run(); }\n",
    )
    .expect("main");
    fs::write(root.join("src/coding.rs"), "pub fn helper() {}\n").expect("coding");
    fs::write(root.join("src/renderer.rs"), "pub struct Renderer;\n")
    .expect("renderer");
    fs::write(root.join("patches.json"), "{ \"patches\": [] }\n").expect("patches");
}

#[test]
fn coding_malformed_import_batch_recovers_grouped_use_tree() {
    let workspace = temp_workspace("grouped_use_tree");
    write_workspace_manifest(&workspace);
    fs::write(
        workspace.join("src/app.rs"),
        "use crate::{\n    coding::\n    renderer::{Renderer,\n};\n\npub fn run() -> Renderer {\n    Renderer\n}\n",
    )
    .expect("app");

    let output = Command::new(env!("CARGO_BIN_EXE_design_cli"))
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            workspace.join("patches.json").to_str().expect("utf8 patch"),
            "--check",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\": \"checked\""), "{stdout}");
    assert!(stdout.contains("\"build_ok\": true"), "{stdout}");
    assert!(!stdout.contains("pub mod"), "{stdout}");

    let telemetry =
        fs::read_to_string(workspace.join(".dbm/telemetry/malformed_import_recovery.json"))
            .expect("malformed import telemetry");
    assert!(telemetry.contains("\"group_normalized\": true"), "{telemetry}");
    assert!(telemetry.contains("\"imports_fixed\":"), "{telemetry}");
}

#[test]
fn coding_malformed_import_batch_preserves_existing_green_imports() {
    let workspace = temp_workspace("preserve_green");
    write_workspace_manifest(&workspace);
    fs::write(
        workspace.join("src/app.rs"),
        "use crate::coding;\nuse crate::{\n    renderer::{Renderer,\n};\n\npub fn run() -> Renderer {\n    coding::helper();\n    Renderer\n}\n",
    )
    .expect("app");

    let output = Command::new(env!("CARGO_BIN_EXE_design_cli"))
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            workspace.join("patches.json").to_str().expect("utf8 patch"),
            "--check",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\": \"checked\""), "{stdout}");

    let telemetry =
        fs::read_to_string(workspace.join(".dbm/telemetry/malformed_import_recovery.json"))
            .expect("malformed import telemetry");
    assert!(telemetry.contains("\"stable_preserved\": true"), "{telemetry}");

    let original = fs::read_to_string(workspace.join("src/app.rs")).expect("app");
    assert!(original.contains("use crate::coding;"), "{original}");
}
