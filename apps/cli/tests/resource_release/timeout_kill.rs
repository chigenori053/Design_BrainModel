use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, create_sandbox_project, run_design_cli_json,
};

#[test]
fn cpu_release_timeout_kill_reaps_children() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    let dir = create_sandbox_project(
        &mut guard,
        "design_cli_timeout_release_timeout",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"timeout_release_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/main.rs",
                "use std::time::Duration;\nfn main() { std::thread::sleep(Duration::from_secs(30)); }\n",
            ),
        ],
    );
    let report = run_design_cli_json(
        &mut guard,
        exe,
        &[
            "run",
            dir.to_str().expect("utf8 dir"),
            "--timeout-ms",
            "50",
            "--json",
        ],
        &[],
    );
    let cpu_release = &report["telemetry"]["cpu_release"];

    assert_eq!(report["status"], "timeout");
    assert_eq!(cpu_release["child_processes_after"], 0);
    assert_eq!(cpu_release["zombie_detected"], false);
    assert!(
        cpu_release["cpu_idle_recovery_ms"]
            .as_u64()
            .expect("idle recovery ms")
            <= 3_000
    );
    guard.force_cleanup();
    assert_scope_recovered(&guard, 2);
}
