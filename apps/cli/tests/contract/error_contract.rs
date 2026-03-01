use serde_json::Value;
use std::process::Command;

#[test]
fn error_json_structure_and_exit_code_2() {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .args(["invalid-cmd"])
        .output()
        .expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let json: Value = serde_json::from_str(&stderr).expect("stderr json");
    assert_eq!(code, 2);
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
}
