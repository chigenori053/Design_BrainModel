use serde_json::Value;
use std::process::Command;

fn run_raw(args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe).args(args).output().expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn analyze_help_exposes_unified_flags_and_hides_legacy_flags() {
    let (code, stdout, _) = run_raw(&["analyze", "--help"]);
    assert_eq!(code, 0, "stdout: {stdout}");
    for flag in [
        "--detailed",
        "--report",
        "--design",
        "--design-json",
        "--lang",
        "--intent",
        "--json",
    ] {
        assert!(stdout.contains(flag), "missing {flag} in {stdout}");
    }
    for legacy in ["--target", "--seed", "--beam-width"] {
        assert!(!stdout.contains(legacy), "unexpected {legacy} in {stdout}");
    }
}

#[test]
fn analyze_detailed_report_combo_uses_unified_renderer() {
    let (code, stdout, stderr) = run_raw(&["analyze", ".", "--detailed", "--report"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("DBM Analyze Report"), "stdout: {stdout}");
    assert!(stdout.contains("[Modules]"), "stdout: {stdout}");
    assert!(stdout.contains("=== Report ==="), "stdout: {stdout}");
}

#[test]
fn analyze_json_takes_priority_over_report_and_design() {
    let (code, stdout, stderr) = run_raw(&["analyze", ".", "--json", "--report", "--design"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let parsed: Value = serde_json::from_str(&stdout).expect("json stdout");
    assert!(parsed.get("decision").is_some(), "stdout: {stdout}");
    assert!(parsed.get("analysis").is_some(), "stdout: {stdout}");
    assert!(!stdout.contains("DBM Analyze Report"), "stdout: {stdout}");
    assert!(!stdout.contains("=== Report ==="), "stdout: {stdout}");
}

#[test]
fn phase_analyze_help_exposes_legacy_flags() {
    let (code, stdout, _) = run_raw(&["phase-analyze", "--help"]);
    assert_eq!(code, 0, "stdout: {stdout}");
    for legacy in ["--target", "--seed", "--beam-width"] {
        assert!(stdout.contains(legacy), "missing {legacy} in {stdout}");
    }
}
