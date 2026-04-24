use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("verification_cli_{name}_{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn trace_replay_diff_and_audit_run() {
    let bin = env!("CARGO_BIN_EXE_verification_cli");
    let dir = temp_dir("happy");
    let trace = dir.join("trace.json");
    let replay = dir.join("trace_replay.json");
    let diff = dir.join("diff.json");
    let audit = dir.join("audit.json");

    assert!(
        Command::new(bin)
            .args(["trace", "--scenario", "rest-api", "--output"])
            .arg(&trace)
            .status()
            .expect("trace")
            .success()
    );
    assert!(
        Command::new(bin)
            .arg("replay")
            .arg(&trace)
            .arg("--output")
            .arg(&replay)
            .status()
            .expect("replay")
            .success()
    );
    assert!(
        Command::new(bin)
            .arg("diff")
            .arg(&trace)
            .arg(&replay)
            .arg("--output")
            .arg(&diff)
            .status()
            .expect("diff")
            .success()
    );
    assert!(
        Command::new(bin)
            .args(["audit", "--scenario", "layered", "--output"])
            .arg(&audit)
            .status()
            .expect("audit")
            .success()
    );

    let diff_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(diff).expect("diff json")).expect("diff parse");
    assert_eq!(diff_report["result"], "deterministic");

    let audit_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(audit).expect("audit json")).expect("audit parse");
    assert_eq!(audit_report["report"]["result"], "deterministic");
    assert!(audit_report["trace"].is_object());
    assert!(audit_report["replay"].is_object());
}

#[test]
fn undefined_scenario_fails() {
    let bin = env!("CARGO_BIN_EXE_verification_cli");
    let dir = temp_dir("bad_scenario");
    let trace = dir.join("trace.json");

    let status = Command::new(bin)
        .args(["trace", "--scenario", "unknown", "--output"])
        .arg(&trace)
        .status()
        .expect("trace");

    assert!(!status.success());
}

#[test]
fn invalid_json_fails() {
    let bin = env!("CARGO_BIN_EXE_verification_cli");
    let dir = temp_dir("bad_json");
    let trace = dir.join("broken.json");
    let output = dir.join("trace_replay.json");
    fs::write(&trace, "{not-json").expect("write broken json");

    let status = Command::new(bin)
        .arg("replay")
        .arg(&trace)
        .arg("--output")
        .arg(&output)
        .status()
        .expect("replay");

    assert!(!status.success());
}
