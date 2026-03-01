use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> (i32, Option<Value>, Option<Value>) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe).args(args).output().expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let out_json = serde_json::from_str(&stdout).ok();
    let err_json = serde_json::from_str(&stderr).ok();
    (code, out_json, err_json)
}

#[test]
fn command_flow_basic() {
    for cmd in ["clear", "adopt", "reject"] {
        let (code, out, _) = run(&[cmd]);
        assert_eq!(code, 0, "failed command: {cmd}");
        let out = out.expect("stdout json");
        assert_eq!(out["schema_version"], "v1");
        assert!(out["data"].is_object());
    }

    let (code, out, _) = run(&["export", "--out", "tmp/out.json"]);
    assert_eq!(code, 0);
    let out = out.expect("stdout json");
    assert_eq!(out["schema_version"], "v1");
    assert_eq!(out["data"]["exported"], true);
}

#[cfg(feature = "ci-heavy")]
#[test]
fn command_flow_heavy_phase1_commands() {
    for cmd in ["analyze", "explain", "simulate"] {
        let (code, out, _) = run(&[cmd, "--beam-width", "1", "--max-steps", "1"]);
        assert_eq!(code, 0, "failed command: {cmd}");
        let out = out.expect("stdout json");
        assert_eq!(out["schema_version"], "v1");
        assert!(out["data"].is_object());
    }
}
