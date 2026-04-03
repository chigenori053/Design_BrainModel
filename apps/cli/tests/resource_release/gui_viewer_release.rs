use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, create_sandbox_project, run_design_cli_json,
};

#[test]
fn cpu_release_gui_viewer_reports_zero_live_loops() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    let dir = create_sandbox_project(
        &mut guard,
        "design_cli_viewer_release_view",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"viewer_release\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod renderer;\npub mod debug;\n"),
            ("src/renderer.rs", "use crate::debug;\npub fn render() {}\n"),
            ("src/debug.rs", "use crate::renderer;\npub fn debug() {}\n"),
        ],
    );
    let report = run_design_cli_json(
        &mut guard,
        exe,
        &[
            "structure",
            "view",
            dir.to_str().expect("utf8 dir"),
            "--3d",
            "--json",
        ],
        &[("DBM_VIEWER_SKIP_OPEN", "1")],
    );

    assert_eq!(report["viewer_loop"]["watcher_count"], 0);
    assert_eq!(report["viewer_loop"]["websocket_count"], 0);
    assert_eq!(report["viewer_loop"]["polling_loop_count"], 0);
    guard.force_cleanup();
    assert_scope_recovered(&guard, 2);
}
