use design_cli::coding::resolve_mutation_target_path;
use design_cli::service::{MutationConstraints, MutationOperation, MutationPlan, MutationStrategy};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_mutation_target_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/adapter")).expect("src adapter");
    fs::create_dir_all(dir.join("src/world")).expect("src world");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/adapter")).expect("fixture adapter");
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mutation_target_ranking\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter/mod.rs"),
        "use crate::world;\npub fn bind() {}\n",
    )
    .expect("adapter");
    fs::write(root.join("src/world/mod.rs"), "pub fn ping() {}\n").expect("world");
    fs::write(
        root.join("tests/fixtures/demo/src/adapter/mod.rs"),
        "use crate::world;\npub fn fixture_only() {}\n",
    )
    .expect("fixture");
}

fn mutation_plan() -> MutationPlan {
    MutationPlan {
        edge_id: "adapter->world".to_string(),
        operation: MutationOperation::RemoveDependency,
        strategy: MutationStrategy::ExtractInterface,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: Some("src/adapter/mod.rs".to_string()),
        snapshot_version: Some("snapshot-v1".to_string()),
        resolver_version: Some("snapshot-v1".to_string()),
    }
}

fn run_cli(workspace: &Path, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run design_cli");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

fn write_mutation_plan(path: &Path) {
    fs::write(
        path,
        r#"{
  "edge_id": "adapter->world",
  "operation": "remove_dependency",
  "strategy": "extract_interface",
  "constraints": {
    "preserve_public_api": true,
    "no_new_cycles": true,
    "target_scope_locked": true
  }
}"#,
    )
    .expect("mutation");
}

#[test]
fn mutation_target_ranking_prefers_production_over_fixture() {
    let root = temp_workspace("production_over_fixture");
    write_workspace(&root);

    let resolved = resolve_mutation_target_path(&root, &mutation_plan())
        .expect("resolve")
        .expect("target");
    assert_eq!(resolved, PathBuf::from("src/adapter/mod.rs"));
}

#[test]
fn mutation_target_ranking_is_deterministic_across_repeated_resolution() {
    let root = temp_workspace("deterministic");
    write_workspace(&root);

    let first = resolve_mutation_target_path(&root, &mutation_plan())
        .expect("resolve")
        .expect("target");
    for _ in 0..100 {
        let current = resolve_mutation_target_path(&root, &mutation_plan())
            .expect("resolve")
            .expect("target");
        assert_eq!(current, first);
    }
}

#[test]
fn mutation_target_ranking_locks_ranked_exact_file_into_apply_resolution() {
    let root = temp_workspace("target_locked");
    write_workspace(&root);
    let mutation = root.join(".dbm/design/mutation.json");
    fs::create_dir_all(mutation.parent().expect("parent")).expect("dbm design dir");
    write_mutation_plan(&mutation);

    let (code, stdout, stderr) = run_cli(
        &root,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--apply",
            "--json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("json");
    let resolution = parsed
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .expect("apply resolution");
    assert_eq!(
        resolution
            .get("resolved_relative_path")
            .and_then(Value::as_str),
        Some("src/adapter/mod.rs"),
        "stdout: {stdout}"
    );
}

#[test]
fn mutation_target_ranking_does_not_override_explicit_target() {
    let root = temp_workspace("explicit_target");
    write_workspace(&root);
    let mutation = root.join(".dbm/design/mutation.json");
    fs::create_dir_all(mutation.parent().expect("parent")).expect("dbm design dir");
    write_mutation_plan(&mutation);

    let (code, stdout, stderr) = run_cli(
        &root,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--target",
            "tests/fixtures/demo/src/adapter/mod.rs",
            "--apply",
            "--json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("json");
    let resolution = parsed
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .expect("apply resolution");
    assert_eq!(
        resolution
            .get("resolved_relative_path")
            .and_then(Value::as_str),
        Some("tests/fixtures/demo/src/adapter/mod.rs"),
        "stdout: {stdout}"
    );
}
