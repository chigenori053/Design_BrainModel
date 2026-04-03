use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, create_sandbox_project, run_design_cli_json,
};

#[test]
fn cpu_release_run_recovers_threads_and_children() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    let dir = create_sandbox_project(
        &mut guard,
        "design_cli_cpu_release_run_ok",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"cpu_release_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/main.rs",
                "fn main() { println!(\"cpu-release-ok\"); }\n",
            ),
        ],
    );
    let report = run_design_cli_json(
        &mut guard,
        exe,
        &["run", dir.to_str().expect("utf8 dir"), "--json"],
        &[],
    );
    let cpu_release = &report["telemetry"]["cpu_release"];

    assert_eq!(report["status"], "success");
    assert_eq!(cpu_release["child_processes_after"], 0);
    assert_eq!(cpu_release["zombie_detected"], false);
    assert_eq!(
        cpu_release["baseline_threads"].as_u64(),
        cpu_release["final_threads"].as_u64()
    );
    assert!(
        cpu_release["cpu_idle_recovery_ms"]
            .as_u64()
            .expect("idle recovery ms")
            <= 3_000
    );
    guard.force_cleanup();
    assert_scope_recovered(&guard, 2);
}

#[test]
fn cpu_release_autonomous_build_recovers_threads_and_children() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    let dir = create_sandbox_project(
        &mut guard,
        "design_cli_cpu_release_autonomous_ok",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"cpu_release_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "src/main.rs",
                "fn main() { println!(\"autonomous-ok\"); }\n",
            ),
        ],
    );
    let report = run_design_cli_json(
        &mut guard,
        exe,
        &[
            "execute",
            "build",
            "--path",
            dir.to_str().expect("utf8 dir"),
            "--json",
        ],
        &[],
    );
    let cpu_release = &report["attempts"][0]["exec_report"]["telemetry"]["cpu_release"];

    assert_eq!(report["status"], "success");
    assert_eq!(report["retry_count"], 0);
    assert_eq!(cpu_release["child_processes_after"], 0);
    assert_eq!(cpu_release["zombie_detected"], false);
    assert_eq!(
        cpu_release["baseline_threads"].as_u64(),
        cpu_release["final_threads"].as_u64()
    );
    guard.force_cleanup();
    assert_scope_recovered(&guard, 2);
}
