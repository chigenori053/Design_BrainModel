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
fn schema_v1_wrapper_structure() {
    let (code, out, _) = run(&["analyze", "--beam-width", "1", "--max-steps", "1"]);
    let out = out.expect("stdout json");
    assert_eq!(code, 0);
    assert_eq!(out["schema_version"], "v1");
    assert!(out["data"].is_object());
    assert!(out["meta"].is_object());
}

#[test]
fn legacy_and_guided_produce_same_schema() {
    let (_, legacy, _) = run(&["analyze", "--beam-width", "1", "--max-steps", "1"]);
    let (_, guided, _) = run(&[
        "analyze",
        "--beam-width",
        "1",
        "--max-steps",
        "1",
        "--hv-guided",
    ]);
    let legacy = legacy.expect("legacy");
    let guided = guided.expect("guided");
    assert_eq!(legacy["schema_version"], guided["schema_version"]);

    let mut l_fields = legacy["data"]
        .as_object()
        .expect("legacy data")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let mut g_fields = guided["data"]
        .as_object()
        .expect("guided data")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    l_fields.sort();
    g_fields.sort();
    assert_eq!(l_fields, g_fields);

    let mut l_meta = legacy["meta"]
        .as_object()
        .expect("legacy meta")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let mut g_meta = guided["meta"]
        .as_object()
        .expect("guided meta")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    l_meta.sort();
    g_meta.sort();
    assert_eq!(l_meta, g_meta);
}
