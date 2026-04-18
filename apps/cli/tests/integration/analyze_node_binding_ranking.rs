use design_cli::commands::analyze::project::{
    AnalyzeMode, AnalyzeOptions, analyze_with_options, render_output,
};
use design_cli::report::Language;
use design_cli::source_index::ModuleSourceIndex;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_analyze_binding_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/adapter")).expect("src adapter");
    fs::create_dir_all(dir.join("src/world")).expect("src world");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/adapter")).expect("fixture adapter");
    dir
}

fn temp_fixture_only_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_analyze_debug_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("src");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/adapter")).expect("fixture adapter");
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"analyze_binding_ranking\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter/mod.rs"),
        "use crate::world;\npub fn bind() { let _ = 1usize; }\n",
    )
    .expect("adapter");
    fs::write(root.join("src/world/mod.rs"), "pub fn ping() {}\n").expect("world");
    fs::write(
        root.join("tests/fixtures/demo/src/adapter/mod.rs"),
        "use crate::world;\npub fn fixture_only() {}\n",
    )
    .expect("fixture");
}

fn write_fixture_only_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"analyze_binding_debug\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").expect("lib");
    fs::write(
        root.join("tests/fixtures/demo/src/adapter/mod.rs"),
        "pub fn fixture_only() {}\n",
    )
    .expect("fixture");
}

fn run_cli(workspace: &Path, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run design_cli");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn design_json_options(path: &Path) -> AnalyzeOptions {
    AnalyzeOptions {
        path: path.display().to_string(),
        mode: AnalyzeMode::Summary,
        report: false,
        design: false,
        language: Language::English,
        intent: None,
        json: false,
        design_json: true,
    }
}

fn adapter_source_path(snapshot: &Value) -> String {
    snapshot
        .get("nodes")
        .and_then(Value::as_array)
        .expect("nodes")
        .iter()
        .find(|node| node.get("logical_name").and_then(Value::as_str) == Some("adapter"))
        .and_then(|node| node.get("source_path"))
        .and_then(Value::as_str)
        .expect("adapter source_path")
        .to_string()
}

#[test]
fn analyze_node_binding_ranking_prefers_production_over_fixture() {
    let root = temp_workspace("production_beats_fixture");
    write_workspace(&root);

    let (code, stdout, stderr) = run_cli(&root, &["analyze", ".", "--design-json"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let snapshot: Value = serde_json::from_str(&stdout).expect("snapshot json");
    assert_eq!(adapter_source_path(&snapshot), "src/adapter/mod.rs");
    assert_eq!(
        snapshot
            .get("graph_binding_debug_hits")
            .and_then(Value::as_u64),
        Some(0),
        "stdout: {stdout}"
    );
    assert_eq!(
        snapshot
            .get("graph_binding_resolution_hits")
            .and_then(Value::as_u64),
        Some(0),
        "stdout: {stdout}"
    );
    assert_eq!(
        snapshot
            .get("fixture_binding_detected")
            .and_then(Value::as_bool),
        Some(false),
        "stdout: {stdout}"
    );
}

#[test]
fn analyze_node_binding_ranking_is_deterministic_for_100_runs() {
    let root = temp_workspace("deterministic");
    write_workspace(&root);

    let options = design_json_options(&root);
    let mut observed = Vec::new();
    for _ in 0..100 {
        let result = analyze_with_options(&options).expect("analyze");
        let rendered = render_output(&result, &options);
        let snapshot: Value = serde_json::from_str(&rendered).expect("snapshot json");
        observed.push(adapter_source_path(&snapshot));
    }

    assert!(observed.iter().all(|path| path == "src/adapter/mod.rs"));
}

#[test]
fn analyze_node_binding_ranking_keeps_downstream_mutation_on_snapshot_path() {
    let root = temp_workspace("downstream_safe");
    write_workspace(&root);

    let (analyze_code, analyze_stdout, analyze_stderr) =
        run_cli(&root, &["analyze", ".", "--design-json"]);
    assert_eq!(analyze_code, 0, "stderr: {analyze_stderr}");
    let snapshot: Value = serde_json::from_str(&analyze_stdout).expect("snapshot json");
    let source_path = adapter_source_path(&snapshot);

    let mutation = json!({
        "edge_id": "adapter->world",
        "operation": "remove_dependency",
        "strategy": "extract_interface",
        "constraints": {
            "preserve_public_api": true,
            "no_new_cycles": true,
            "target_scope_locked": true
        },
        "source_path": source_path,
        "snapshot_version": "snapshot-v1",
        "resolver_version": "3"
    });
    fs::create_dir_all(root.join(".dbm/design")).expect("design dir");
    fs::write(
        root.join(".dbm/design/mutation.json"),
        serde_json::to_string_pretty(&mutation).expect("mutation json"),
    )
    .expect("write mutation");

    let (code, stdout, stderr) = run_cli(
        &root,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--check",
            "--json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("coding json");
    assert_eq!(
        report
            .get("execution")
            .and_then(|value| value.get("degraded_resolution_hits"))
            .and_then(Value::as_u64),
        Some(0),
        "stdout: {stdout}"
    );
    assert_eq!(
        report
            .get("execution")
            .and_then(|value| value.get("canonical_target_path"))
            .and_then(Value::as_str),
        Some("src/adapter/mod.rs"),
        "stdout: {stdout}"
    );
}

#[test]
fn analyze_node_binding_ranking_allows_fixture_only_in_debug_fallback() {
    let root = temp_fixture_only_workspace("debug_fallback");
    write_fixture_only_workspace(&root);

    let index = ModuleSourceIndex::build(&root).expect("index");
    assert!(index.bind_graph_node("adapter").is_none());

    let fallback = index
        .bind_graph_node_debug_fallback("adapter")
        .expect("debug fallback");
    assert_eq!(
        fallback.1,
        PathBuf::from("tests/fixtures/demo/src/adapter/mod.rs")
    );
}
