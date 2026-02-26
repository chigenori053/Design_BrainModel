use serde_json::Value;
use std::process::Command;

fn run_cli(args: &[&str]) -> (i32, Option<Value>, Option<Value>) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("failed to run design cli");

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let out_json = serde_json::from_str(&stdout).ok();
    let err_json = serde_json::from_str(&stderr).ok();
    (code, out_json, err_json)
}

#[test]
fn schema_v1_wrapper_structure() {
    let (code, out, _) = run_cli(&["analyze"]);
    let out = out.expect("stdout json");
    assert_eq!(code, 0);
    assert_eq!(out["schema_version"], "v1");
    assert!(out["data"].is_object());
    assert!(out["meta"].is_object());
}

#[test]
fn error_json_structure() {
    let (code, _, err) = run_cli(&["unknown-cmd"]);
    let err = err.expect("stderr json");
    assert_eq!(code, 2);
    assert!(err["error"]["code"].is_string());
    assert!(err["error"]["message"].is_string());
    assert!(err["error"].get("details").is_some());
}

#[test]
fn legacy_and_guided_produce_same_schema() {
    let (_, legacy, _) = run_cli(&["analyze"]);
    let (_, guided, _) = run_cli(&["analyze", "--hv-guided"]);
    let legacy = legacy.expect("legacy json");
    let guided = guided.expect("guided json");

    let l_data = legacy["data"].as_object().expect("legacy data object");
    let g_data = guided["data"].as_object().expect("guided data object");
    let mut l_keys = l_data.keys().cloned().collect::<Vec<_>>();
    let mut g_keys = g_data.keys().cloned().collect::<Vec<_>>();
    l_keys.sort();
    g_keys.sort();
    assert_eq!(l_keys, g_keys);
}
