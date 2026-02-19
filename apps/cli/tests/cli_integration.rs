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

#[test]
fn analyze_json_snapshot_matches_spec() {
    let store_dir = unique_store_dir("analyze_json");
    let (ok, stdout, _) = run_cli(
        &store_dir,
        &["analyze", "high-level architecture", "--json"],
    );
    assert!(ok);
    assert!(stdout.starts_with("{\n    \"type\": \"analyze\",\n"));
    assert!(stdout.contains("\"concept_id\": \"C1\""));
    assert!(stdout.contains("\"abstraction\": "));
    assert!(stdout.contains("\"abstraction_label\": \""));
}

#[test]
fn explain_json_snapshot_matches_spec() {
    let store_dir = unique_store_dir("explain_json");
    let _ = run_cli(&store_dir, &["analyze", "design architecture concept"]);
    let (ok, stdout, _) = run_cli(&store_dir, &["explain", "C1", "--json"]);
    assert!(ok);
    assert_eq!(stdout.lines().next(), Some("{"));
    assert!(stdout.contains("\"type\": \"explain\""));
    assert!(stdout.contains("\"concept_id\": \"C1\""));
    assert!(stdout.contains("\"summary\": "));
    assert!(stdout.contains("\"reasoning\": "));
    assert!(stdout.contains("\"abstraction_note\": "));
}

#[test]
fn compare_json_snapshot_matches_spec() {
    let store_dir = unique_store_dir("compare_json");
    let _ = run_cli(&store_dir, &["analyze", "design architecture concept"]);
    let _ = run_cli(&store_dir, &["analyze", "concrete detail"]);
    let (ok, stdout, _) = run_cli(&store_dir, &["compare", "C1", "C2", "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"type\": \"compare\""));
    assert!(stdout.contains("\"concept_a\": \"C1\""));
    assert!(stdout.contains("\"concept_b\": \"C2\""));
    assert!(stdout.contains("\"semantic_similarity\": "));
    assert!(stdout.contains("\"structural_similarity\": "));
    assert!(stdout.contains("\"abstraction_difference\": "));
    assert!(stdout.contains("\"alignment_label\": "));
}

#[test]
fn multi_json_snapshot_matches_spec() {
    let store_dir = unique_store_dir("multi_json");
    let _ = run_cli(&store_dir, &["analyze", "architecture abstraction model"]);
    let _ = run_cli(&store_dir, &["analyze", "system design pattern"]);
    let (ok, stdout, _) = run_cli(&store_dir, &["multi", "C1", "C2", "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"type\": \"multi\""));
    assert!(stdout.contains("\"concept_ids\": [\"C1\", \"C2\"]"));
    assert!(stdout.contains("\"mean_resonance\": "));
    assert!(stdout.contains("\"mean_abstraction\": "));
    assert!(stdout.contains("\"conflict_pairs\": "));
    assert!(stdout.contains("\"coherence_label\": "));
    assert!(stdout.contains("\"abstraction_label\": "));
}

#[test]
fn recommend_json_snapshot_matches_spec() {
    let store_dir = unique_store_dir("recommend_json");
    let _ = run_cli(&store_dir, &["analyze", "architecture abstraction model"]);
    let _ = run_cli(&store_dir, &["analyze", "system design pattern"]);
    let (ok, stdout, _) = run_cli(&store_dir, &["recommend", "C1", "--top", "3", "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"type\": \"recommend\""));
    assert!(stdout.contains("\"query\": \"C1\""));
    assert!(stdout.contains("\"recommendations\": ["));
    assert!(stdout.contains("\"target\": \"C2\""));
    assert!(stdout.contains("\"action\": "));
    assert!(stdout.contains("\"score\": "));
    assert!(stdout.contains("\"summary\": "));
}

#[test]
fn json_output_uses_four_space_indent_and_stable_key_order() {
    let store_dir = unique_store_dir("json_indent_order");
    let (ok, stdout, _) = run_cli(
        &store_dir,
        &["analyze", "architecture abstraction", "--json"],
    );
    assert!(ok);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "{");
    assert!(lines[1].starts_with("    \"type\""));
    assert!(lines[2].starts_with("    \"concept_id\""));
    assert!(lines[3].starts_with("    \"abstraction\""));
    assert!(lines[4].starts_with("    \"abstraction_label\""));
}

#[test]
fn json_numbers_are_rounded_to_two_decimals() {
    let store_dir = unique_store_dir("json_rounding");
    let _ = run_cli(&store_dir, &["analyze", "architecture abstraction model"]);
    let _ = run_cli(&store_dir, &["analyze", "system detail"]);
    let (ok, stdout, _) = run_cli(&store_dir, &["compare", "C1", "C2", "--json"]);
    assert!(ok);
    for key in [
        "\"semantic_similarity\": ",
        "\"structural_similarity\": ",
        "\"abstraction_difference\": ",
    ] {
        let line = stdout
            .lines()
            .find(|line| line.contains(key))
            .expect("metric line");
        let value = line
            .split(':')
            .nth(1)
            .expect("value")
            .trim()
            .trim_end_matches(',');
        let dot = value.find('.').expect("decimal point");
        assert_eq!(value.len() - dot - 1, 2);
    }
}

#[test]
fn text_mode_output_remains_unchanged() {
    let store_dir = unique_store_dir("text_unchanged");
    let (ok, stdout, _) = run_cli(
        &store_dir,
        &["analyze", "Design abstraction improves architecture"],
    );
    assert!(ok);
    assert!(stdout.starts_with("[Concept Created]\nID: C1\nAbstraction: "));
}

#[test]
fn json_mode_is_deterministic() {
    let store_dir = unique_store_dir("json_deterministic");
    let _ = run_cli(&store_dir, &["analyze", "deterministic concept"]);
    let (_, first, _) = run_cli(&store_dir, &["explain", "C1", "--json"]);
    let (_, second, _) = run_cli(&store_dir, &["explain", "C1", "--json"]);
    assert_eq!(first, second);
}
