use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_structure_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"structure_view\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
fn gui_render_modes_export_ir_and_report_expected_launch_targets() {
    for (name, mode_flag, expected_mode, expected_launch_marker) in [
        ("2d", "--2d", "\"mode\": \"2d\"", "\"launch_url\""),
        ("3d", "--3d", "\"mode\": \"3d\"", "embedded://viewer_core"),
    ] {
        let dir = temp_project(name);
        let (code, stdout, _) = run(&[
            "structure",
            "view",
            dir.to_str().unwrap(),
            mode_flag,
            "--json",
        ]);
        assert_eq!(code, 0);
        assert!(stdout.contains(expected_mode), "stdout: {stdout}");
        assert!(stdout.contains(expected_launch_marker), "stdout: {stdout}");
        assert!(dir.join(".dbm/structure_view.json").exists());
    }
    let dir = temp_project("3d_mode_suffix");
    let (code, stdout, _) = run(&["structure", "view", dir.to_str().unwrap(), "--3d", "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("mode=3d"));
}

#[test]
fn ipc_roundtrip_dispatches_refactor_and_refreshes_ir() {
    let dir = temp_project("dispatch");
    let event_path = dir.join("event.json");
    fs::write(
        &event_path,
        r#"{"action":"refactor","target":"cycle","node":"renderer","project_root":null}"#,
    )
    .expect("write event");
    let (code, stdout, _) = run(&[
        "structure",
        "dispatch",
        dir.to_str().unwrap(),
        "--event",
        event_path.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"command_kind\": \"Refactor\""));
    assert!(stdout.contains("\"preview\""));
}

#[test]
fn gui_dispatch_defaults_to_canonical_gui_action_file() {
    let dir = temp_project("dispatch_default");
    let action_path = dir.join(".dbm/gui_action.json");
    fs::create_dir_all(action_path.parent().expect("parent")).expect("mkdir");
    fs::write(
        &action_path,
        r#"{"action":"refactor","target":"cycle","node":"renderer","project_root":null}"#,
    )
    .expect("write event");
    let (code, stdout, _) = run(&["structure", "dispatch", dir.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0);
    assert!(stdout.contains(".dbm/gui_action.json"));
    assert!(stdout.contains("\"command_kind\": \"Refactor\""));
}
