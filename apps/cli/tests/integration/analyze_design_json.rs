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
fn analyze_json_schema_remains_unchanged() {
    let (code, stdout, stderr) = run_raw(&["analyze", ".", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("json stdout");
    assert!(parsed.get("modules").and_then(Value::as_u64).is_some());
    assert!(parsed.get("cycles").and_then(Value::as_u64).is_some());
    let violations = parsed
        .get("violations")
        .and_then(Value::as_array)
        .expect("violations array");
    assert!(
        violations.iter().all(|entry| entry.as_str().is_some()),
        "stdout: {stdout}"
    );
    assert!(parsed.get("nodes").is_none(), "stdout: {stdout}");
    assert!(parsed.get("edges").is_none(), "stdout: {stdout}");
}

#[test]
fn analyze_design_json_emits_snapshot_lists() {
    let (code, stdout, stderr) = run_raw(&["analyze", ".", "--design-json"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("json stdout");
    assert!(parsed.get("edges").and_then(Value::as_array).is_some());
    assert!(parsed.get("cycles").and_then(Value::as_array).is_some());
    let violations = parsed
        .get("violations")
        .and_then(Value::as_array)
        .expect("violations array");
    assert!(
        violations
            .iter()
            .all(|entry| entry.get("violation_type").is_some()),
        "stdout: {stdout}"
    );
}

#[test]
fn analyze_design_json_contains_deterministic_adapter_world_edge() {
    let (code, stdout, stderr) = run_raw(&["analyze", ".", "--design-json"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("json stdout");
    let edge = parsed
        .get("edges")
        .and_then(Value::as_array)
        .expect("edges array")
        .iter()
        .find(|edge| edge.get("id").and_then(Value::as_str) == Some("adapter->world"))
        .expect("adapter->world edge");
    assert_eq!(edge.get("from").and_then(Value::as_str), Some("adapter"));
    assert_eq!(edge.get("to").and_then(Value::as_str), Some("world"));
}
