use std::fs;

use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, create_sandbox_project, run_design_cli,
};

#[test]
fn structure_dispatch_preview_is_reproducible_for_multi_select() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    let dir = create_sandbox_project(
        &mut guard,
        "design_cli_preview_exhaustive_preview",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"structure_preview_exhaustive\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod renderer;\npub mod debug;\n"),
            ("src/renderer.rs", "use crate::debug;\npub fn render() {}\n"),
            ("src/debug.rs", "use crate::renderer;\npub fn debug() {}\n"),
        ],
    );
    let event_path = dir.join("preview_event.json");
    fs::write(
        &event_path,
        r#"{
  "action":"refactor",
  "target":"auto",
  "node":"renderer",
  "project_root":null,
  "selected_nodes":["renderer","debug"],
  "selected_edges":[{"from":"renderer","to":"debug"}],
  "mode":"preview"
}"#,
    )
    .expect("write preview event");

    let first = run_design_cli(
        &mut guard,
        exe,
        &[
            "structure",
            "dispatch",
            dir.to_str().unwrap(),
            "--event",
            event_path.to_str().unwrap(),
            "--json",
        ],
        &[("DBM_VIEWER_SKIP_OPEN", "1")],
    );
    let second = run_design_cli(
        &mut guard,
        exe,
        &[
            "structure",
            "dispatch",
            dir.to_str().unwrap(),
            "--event",
            event_path.to_str().unwrap(),
            "--json",
        ],
        &[("DBM_VIEWER_SKIP_OPEN", "1")],
    );
    assert_eq!(
        first.status.code().unwrap_or(-1),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        second.status.code().unwrap_or(-1),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let first = String::from_utf8_lossy(&first.stdout).to_string();
    let second = String::from_utf8_lossy(&second.stdout).to_string();

    assert!(first.contains("\"selection_mode\": \"multi\""));
    assert!(first.contains("\"candidates\""));
    assert!(first.contains("\"heatmap\""));
    assert_eq!(first, second);
    guard.force_cleanup();
    assert_scope_recovered(&guard, 2);
}
