use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_sandbox_copy_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("src");
    dir
}

#[test]
fn coding_sandbox_copy_ignores_incremental_artifacts() {
    let workspace = temp_workspace("incremental");
    fs::create_dir_all(workspace.join("target/debug/incremental")).expect("incremental dir");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"sandbox_copy_guard\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(workspace.join("src/main.rs"), "fn main() {}\n").expect("main");
    fs::write(
        workspace.join("target/debug/incremental/foo.part.bin"),
        b"partial artifact",
    )
    .expect("part bin");

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(["coding", ".", "--check"])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let telemetry = fs::read_to_string(workspace.join(".dbm/telemetry/sandbox_copy.json"))
        .expect("sandbox telemetry");
    assert!(telemetry.contains("\"target\""), "{telemetry}");
    assert!(telemetry.contains("*.part.bin skipped"), "{telemetry}");
}
