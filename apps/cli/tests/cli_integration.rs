use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_store_dir(test_name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("design_cli_{test_name}_{nanos}"))
        .to_string_lossy()
        .to_string()
}

fn run_cli(store_dir: &str, args: &[&str]) -> (bool, String, String) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .args(args)
        .env("DESIGN_STORE_DIR", store_dir)
        .output()
        .expect("failed to run design cli");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn analyze_then_explain_works() {
    let store_dir = unique_store_dir("analyze_then_explain");

    let (ok, stdout, _) = run_cli(
        &store_dir,
        &["analyze", "Design abstraction improves architecture"],
    );
    assert!(ok);
    assert!(stdout.contains("[Concept Created]"));
    assert!(stdout.contains("ID: C1"));

    let (ok, stdout, _) = run_cli(&store_dir, &["explain", "C1"]);
    assert!(ok);
    assert!(stdout.contains("Summary:"));
    assert!(stdout.contains("Reasoning:"));
    assert!(stdout.contains("Abstraction:"));
}

#[test]
fn unknown_id_returns_error() {
    let store_dir = unique_store_dir("unknown_id");
    let (ok, _, stderr) = run_cli(&store_dir, &["explain", "C999"]);
    assert!(!ok);
    assert!(stderr.contains("Concept not found"));
}

#[test]
fn multi_deduplicates_ids() {
    let store_dir = unique_store_dir("multi_dedup");

    let _ = run_cli(&store_dir, &["analyze", "alpha design structure"]);
    let _ = run_cli(&store_dir, &["analyze", "beta architecture pattern"]);

    let (ok, stdout, _) = run_cli(&store_dir, &["multi", "C1", "C1", "C2"]);
    assert!(ok);
    assert!(stdout.contains("Structural Analysis:"));
    assert!(stdout.contains("Conflict Analysis:"));
}

#[test]
fn recommend_clamps_top_k() {
    let store_dir = unique_store_dir("recommend_topk");

    let _ = run_cli(&store_dir, &["analyze", "one structure"]);
    let _ = run_cli(&store_dir, &["analyze", "two structure"]);
    let _ = run_cli(&store_dir, &["analyze", "three structure"]);

    let (ok, stdout, _) = run_cli(&store_dir, &["recommend", "C1", "--top", "99"]);
    assert!(ok);
    assert!(stdout.contains("[Recommendations]"));

    let lines = stdout
        .lines()
        .filter(|line| line.starts_with("1.") || line.starts_with("2.") || line.starts_with("3."))
        .count();
    assert_eq!(lines, 2);
}

#[test]
fn deterministic_output_for_same_query() {
    let store_dir = unique_store_dir("deterministic");

    let _ = run_cli(&store_dir, &["analyze", "deterministic concept"]);

    let (_, first, _) = run_cli(&store_dir, &["explain", "C1"]);
    let (_, second, _) = run_cli(&store_dir, &["explain", "C1"]);
    assert_eq!(first, second);
}

#[test]
fn output_headers_fixed() {
    let store_dir = unique_store_dir("headers");

    let _ = run_cli(&store_dir, &["analyze", "compare header sample"]);
    let _ = run_cli(&store_dir, &["analyze", "compare header sample two"]);

    let (ok, stdout, _) = run_cli(&store_dir, &["compare", "C1", "C2"]);
    assert!(ok);
    assert!(stdout.starts_with("Summary:\n"));
}

#[test]
fn binary_name_is_design() {
    let exe = env!("CARGO_BIN_EXE_design");
    assert!(Path::new(exe).exists());
}
