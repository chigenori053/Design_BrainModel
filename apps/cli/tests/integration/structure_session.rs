use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_session_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"structure_session\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(
        dir.join("src/lib.rs"),
        "pub mod renderer;\npub mod debug;\n",
    )
    .expect("write lib");
    fs::write(
        dir.join("src/renderer.rs"),
        "use crate::debug;\npub fn render() {}\n",
    )
    .expect("write renderer");
    fs::write(
        dir.join("src/debug.rs"),
        "use crate::renderer;\npub fn debug() {}\n",
    )
    .expect("write debug");
    dir
}

fn run(args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .env("DBM_VIEWER_SKIP_OPEN", "1")
        .args(args)
        .output()
        .expect("run design");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn structure_session_attach_creates_versioned_ir() {
    let dir = temp_project("attach");
    let (code, stdout, _) = run(&["structure", "session", dir.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"session_id\""));
    let ir = fs::read_to_string(dir.join(".dbm/structure_view.json")).expect("read ir");
    assert!(ir.contains("\"version\": 2"));
}

#[test]
fn structure_dispatch_updates_design_docs_and_supports_undo_redo() {
    let dir = temp_project("undo");
    let event_path = dir.join("event.json");
    fs::write(
        &event_path,
        r#"{"action":"refactor","target":"cycle","node":"renderer","project_root":null}"#,
    )
    .expect("write event");

    let (dispatch_code, dispatch_stdout, _) = run(&[
        "structure",
        "dispatch",
        dir.to_str().unwrap(),
        "--event",
        event_path.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(dispatch_code, 0);
    assert!(dispatch_stdout.contains("\"session_id\""));
    assert!(
        fs::read_to_string(dir.join("design.md"))
            .expect("design")
            .contains("Architecture Delta")
    );
    assert!(
        fs::read_to_string(dir.join("report.md"))
            .expect("report")
            .contains("Removed cycle")
    );

    let (undo_code, undo_stdout, _) = run(&["structure", "undo", dir.to_str().unwrap(), "--json"]);
    assert_eq!(undo_code, 0);
    assert!(undo_stdout.contains("\"redo_depth\": 1"));

    let (redo_code, redo_stdout, _) = run(&["structure", "redo", dir.to_str().unwrap(), "--json"]);
    assert_eq!(redo_code, 0);
    assert!(redo_stdout.contains("\"history_depth\": 1"));
}
