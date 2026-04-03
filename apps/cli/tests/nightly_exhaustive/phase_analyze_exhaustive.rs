use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::test_support::resource_guard::{
    TestScopeGuard, assert_scope_recovered, run_design_cli, unique_sandbox_dir,
};

fn run(guard: &mut TestScopeGuard, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = run_design_cli(guard, exe, args, &[]);
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn phase_analyze_human_coherence_dump_is_still_available() {
    let mut guard = TestScopeGuard::new();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let temp_dir = unique_sandbox_dir("human_coherence_analysis");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    guard.register_sandbox(temp_dir.clone());
    let out_path = temp_dir.join(format!("{ts}.json"));
    let out_path_s = out_path.to_string_lossy().to_string();

    let (code, stdout, stderr) = run(
        &mut guard,
        &[
            "phase-analyze",
            "--beam-width",
            "5",
            "--max-steps",
            "25",
            "--human-coherence",
            "--dump-analysis",
            &out_path_s,
        ],
    );
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");
    let out: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(out["schema_version"], "v1");

    let raw = fs::read_to_string(&out_path).expect("dump file exists");
    let parsed: Value = serde_json::from_str(&raw).expect("dump json");
    let corr = parsed["report"]["corr_hc_total"]
        .as_f64()
        .expect("corr_hc_total");
    assert!(corr.is_finite());
    assert!(corr < 0.65, "corr_hc_total must be < 0.65, got {corr}");

    guard.force_cleanup();
    assert_scope_recovered(&guard, 1);
}
