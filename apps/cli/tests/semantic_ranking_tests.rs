use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> (i32, Value) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe).args(args).output().expect("run design");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let out_json = serde_json::from_str(&stdout).expect("stdout json");
    (code, out_json)
}

#[test]
fn semantic_ranking_is_deterministic() {
    let args = [
        "analyze",
        "--beam-width",
        "2",
        "--max-steps",
        "2",
        "--semantic-rank",
    ];
    let (c1, a) = run(&args);
    let (c2, b) = run(&args);
    assert_eq!(c1, 0);
    assert_eq!(c2, 0);
    assert_eq!(a["data"]["cases"], b["data"]["cases"]);
}

#[test]
fn ranking_does_not_change_frontier_size() {
    let args = ["analyze", "--beam-width", "2", "--max-steps", "2"];
    let (c1, base) = run(&args);
    let (c2, ranked) = run(&[
        "analyze",
        "--beam-width",
        "2",
        "--max-steps",
        "2",
        "--semantic-rank",
    ]);
    assert_eq!(c1, 0);
    assert_eq!(c2, 0);
    assert_eq!(base["data"]["frontier_size"], ranked["data"]["frontier_size"]);
}

#[test]
fn ranking_respects_pareto_rank() {
    let (code, out) = run(&[
        "analyze",
        "--beam-width",
        "3",
        "--max-steps",
        "2",
        "--semantic-rank",
    ]);
    assert_eq!(code, 0);
    let cases = out["data"]["cases"].as_array().expect("cases array");
    let ranks = cases
        .iter()
        .map(|c| c["pareto_rank"].as_u64().expect("pareto_rank"))
        .collect::<Vec<_>>();
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn schema_v1_unchanged() {
    let (code, out) = run(&[
        "analyze",
        "--beam-width",
        "1",
        "--max-steps",
        "1",
        "--semantic-rank",
    ]);
    assert_eq!(code, 0);
    assert_eq!(out["schema_version"], "v1");
}
