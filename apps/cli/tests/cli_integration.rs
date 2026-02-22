use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::Value;

fn unique_store_dir(test_name: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("design_v1_{test_name}_{nanos}")).to_string_lossy().to_string()
}

fn run_cli(store_dir: &str, args: &[&str]) -> (i32, Value) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .args(args)
        .env("DESIGN_STORE_DIR", store_dir)
        .output()
        .expect("failed to run design cli");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    
    let code = out.status.code().unwrap_or(-1);
    let json: Value = if code == 0 {
        serde_json::from_str(&stdout).unwrap_or(Value::Null)
    } else {
        serde_json::from_str(&stderr).unwrap_or(Value::Null)
    };

    (code, json)
}

#[test]
fn schema_v1_wrapper_structure() {
    let store = unique_store_dir("wrapper");
    let (code, json) = run_cli(&store, &["--json", "analyze", "test text"]);
    
    assert_eq!(code, 0);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], "1.0");
    assert_eq!(json["command"], "analyze");
    assert!(json["data"].is_object());
    assert!(json["error"].is_null());
}

#[test]
fn analyze_data_schema() {
    let store = unique_store_dir("analyze");
    let (code, json) = run_cli(&store, &["--json", "analyze", "security is important"]);
    
    assert_eq!(code, 0);
    let data = &json["data"];
    assert!(data["l1_count"].is_number());
    assert!(data["l2_count"].is_number());
    assert!(data["stability_score"].is_number());
    assert!(data["ambiguity_score"].is_number());
    assert!(data["snapshot"]["l1_hash"].is_string());
    assert_eq!(data["snapshot"]["version"], 2);
}

#[test]
fn explain_data_schema() {
    let store = unique_store_dir("explain");
    let _ = run_cli(&store, &["analyze", "goal: fast system"]);
    let (code, json) = run_cli(&store, &["--json", "explain"]);
    
    assert_eq!(code, 0);
    let data = &json["data"];
    assert!(data["stability_label"].is_string());
    assert!(data["ambiguity_label"].is_string());
    assert!(data["template_id"].is_string());
}

#[test]
fn error_json_structure() {
    let store = unique_store_dir("error");
    let (code, json) = run_cli(&store, &["--json", "invalid-cmd"]);
    
    assert_eq!(code, 3);
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], 3);
    assert_eq!(json["error"]["type"], "InvalidCommand");
    assert!(json["error"]["message"].is_string());
}

#[test]
fn session_json_file_schema() {
    let store = unique_store_dir("session_file");
    let _ = run_cli(&store, &["analyze", "session test"]);
    let _ = run_cli(&store, &["--session", "mysess", "session", "save"]);
    
    let path = std::path::Path::new(&store).join("session_mysess.json");
    assert!(path.exists());
    
    let raw = std::fs::read_to_string(path).unwrap();
    let json: Value = serde_json::from_str(&raw).unwrap();
    
    assert_eq!(json["schema_version"], "1.0");
    assert_eq!(json["snapshot_version"], 2);
    assert_eq!(json["id"], "mysess");
    assert!(json["l1_units"].is_array());
    assert!(json["l2_units"].is_array());
    assert!(json["snapshot"]["l1_hash"].is_string());
}
