use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> (i32, Option<Value>, Option<Value>) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
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

#[test]
fn natural_language_routes_to_analyze_with_target() {
    let (code, out, _) = run(&["このプロジェクトを解析して"]);
    assert_eq!(code, 0);
    let out = out.expect("stdout json");
    assert_eq!(out["schema_version"], "v1");
    assert_eq!(out["data"]["target"], "./project");
}

#[test]
fn slash_command_is_accepted() {
    let (code, out, _) = run(&["/analyze", "./project"]);
    assert_eq!(code, 0);
    let out = out.expect("stdout json");
    assert_eq!(out["schema_version"], "v1");
    assert_eq!(out["meta"]["command"], "analyze");
}

#[test]
fn natural_language_missing_target_returns_error() {
    let (code, _, err) = run(&["解析して"]);
    assert_eq!(code, 2);
    let err = err.expect("stderr json");
    let message = err["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("対象が指定されていません"));
}

#[test]
fn hybrid_input_routes_command_and_fills_target() {
    let (code, out, _) = run(&["これを /analyze して"]);
    assert_eq!(code, 0);
    let out = out.expect("stdout json");
    assert_eq!(out["schema_version"], "v1");
    assert_eq!(out["data"]["target"], "./project");
}

#[cfg(feature = "ci-heavy")]
#[test]
fn command_flow_heavy_phase1_commands() {
    for cmd in ["phase-analyze", "explain", "simulate"] {
        let (code, out, _) = run(&[cmd, "--beam-width", "1", "--max-steps", "1"]);
        assert_eq!(code, 0, "failed command: {cmd}");
        let out = out.expect("stdout json");
        assert_eq!(out["schema_version"], "v1");
        assert!(out["data"].is_object());
    }
}
