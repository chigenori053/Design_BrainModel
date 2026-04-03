use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe).args(args).output().expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn phase_analyze_schema_v1_wrapper_structure() {
    let (code, stdout, stderr) = run(&["phase-analyze", "--beam-width", "1", "--max-steps", "1"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let out: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(out["schema_version"], "v1");
    assert!(out["data"].is_object());
    assert!(out["meta"].is_object());
}

#[test]
fn phase_analyze_legacy_and_guided_produce_same_schema() {
    let (legacy_code, legacy, _) = run(&["phase-analyze", "--beam-width", "1", "--max-steps", "1"]);
    let (guided_code, guided, _) = run(&[
        "phase-analyze",
        "--beam-width",
        "1",
        "--max-steps",
        "1",
        "--hv-guided",
    ]);
    assert_eq!(legacy_code, 0, "legacy stdout: {legacy}");
    assert_eq!(guided_code, 0, "guided stdout: {guided}");
    let legacy: Value = serde_json::from_str(&legacy).expect("legacy json");
    let guided: Value = serde_json::from_str(&guided).expect("guided json");
    assert_eq!(legacy["schema_version"], guided["schema_version"]);

    let mut legacy_data = legacy["data"]
        .as_object()
        .expect("legacy data")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let mut guided_data = guided["data"]
        .as_object()
        .expect("guided data")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    legacy_data.sort();
    guided_data.sort();
    assert_eq!(legacy_data, guided_data);
}
