use serde_json::Value;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(args: &[&str]) -> (i32, Value) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe).args(args).output().expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let out_json = serde_json::from_str(&stdout).expect("stdout json");
    (code, out_json)
}

#[test]
fn correlation_below_threshold() {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let out_path = std::env::temp_dir().join(format!("human_coherence_analysis_{ts}.json"));
    let out_path_s = out_path.to_string_lossy().to_string();

    let (code, out) = run(&[
        "analyze",
        "--beam-width",
        "5",
        "--max-steps",
        "25",
        "--human-coherence",
        "--dump-analysis",
        &out_path_s,
    ]);
    assert_eq!(code, 0);
    assert_eq!(out["schema_version"], "v1");

    let raw = fs::read_to_string(&out_path).expect("dump file exists");
    let parsed: Value = serde_json::from_str(&raw).expect("dump json");
    let corr = parsed["report"]["corr_hc_total"]
        .as_f64()
        .expect("corr_hc_total");
    assert!(corr.is_finite());
    assert!(corr < 0.65, "corr_hc_total must be < 0.65, got {corr}");

    let _ = fs::remove_file(&out_path);
}
