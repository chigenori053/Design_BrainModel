use serde_json::Value;

use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, run_design_cli,
};

fn run(
    guard: &mut TestScopeGuard,
    exe: &str,
    args: &[&str],
) -> (i32, Option<Value>, Option<Value>) {
    let out = run_design_cli(guard, exe, args, &[]);
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let out_json = serde_json::from_str(&stdout).ok();
    let err_json = serde_json::from_str(&stderr).ok();
    (code, out_json, err_json)
}

#[test]
fn command_flow_heavy_phase1_commands() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut guard = TestScopeGuard::new();
    for cmd in ["phase-analyze", "explain"] {
        let (code, out, _) = run(
            &mut guard,
            exe,
            &[cmd, "--beam-width", "1", "--max-steps", "1"],
        );
        assert_eq!(code, 0, "failed command: {cmd}");
        let out = out.expect("stdout json");
        assert_eq!(out["schema_version"], "v1");
        assert!(out["data"].is_object());
    }
    guard.force_cleanup();
    assert_scope_recovered(&guard, 1);
}
