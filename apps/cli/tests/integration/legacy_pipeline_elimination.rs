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
    let dir = std::env::temp_dir().join(format!("design_cli_legacy_cleanup_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/adapter")).expect("src adapter");
    fs::create_dir_all(dir.join("src/world")).expect("src world");
    fs::create_dir_all(dir.join("src/ports")).expect("src ports");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/adapter")).expect("fixture adapter");
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"legacy_pipeline_elimination\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod world;\npub mod ports;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter/mod.rs"),
        "use crate::world;\npub fn bind() { let _ = 1usize; }\n",
    )
    .expect("adapter");
    fs::write(root.join("src/world/mod.rs"), "pub fn ping() {}\n").expect("world");
    fs::write(root.join("src/ports/mod.rs"), "pub fn ping() {}\n").expect("ports");
    fs::write(
        root.join("tests/fixtures/demo/src/adapter/mod.rs"),
        "use crate::world;\npub fn fixture_only() {}\n",
    )
    .expect("fixture");
}

fn plan_with_source(source_path: Option<&str>, stale: bool) -> MutationPlan {
    MutationPlan {
        edge_id: "adapter->world".to_string(),
        operation: MutationOperation::RemoveDependency,
        strategy: MutationStrategy::ExtractInterface,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: source_path.map(ToString::to_string),
        snapshot_version: Some(if stale { "snapshot-v0" } else { "snapshot-v1" }.to_string()),
        resolver_version: Some(if stale { "2" } else { "3" }.to_string()),
    }
}

fn write_mutation_plan(path: &Path, plan: &MutationPlan) {
    fs::create_dir_all(path.parent().expect("parent")).expect("design dir");
    fs::write(path, serde_json::to_string_pretty(plan).expect("plan json")).expect("plan write");
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

#[test]
fn legacy_pipeline_elimination_prefers_snapshot_source_path() {
    let root = temp_workspace("snapshot_only");
    write_workspace(&root);
    let resolved =
        resolve_mutation_target_path(&root, &plan_with_source(Some("src/adapter/mod.rs"), false))
            .expect("resolve")
            .expect("target");
    assert_eq!(resolved, PathBuf::from("src/adapter/mod.rs"));
}

#[test]
fn legacy_pipeline_elimination_keeps_check_apply_target_identical() {
    let root = temp_workspace("no_split_resolver");
    write_workspace(&root);
    write_mutation_plan(
        &root.join(".dbm/design/mutation.json"),
        &plan_with_source(Some("src/adapter/mod.rs"), false),
    );

    let (check_code, check_stdout, check_stderr) = run_cli(
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
    assert_eq!(check_code, 0, "stderr: {check_stderr}");
    let check: Value = serde_json::from_str(&check_stdout).expect("check json");
    let preview_target = check
        .get("execution")
        .and_then(|value| value.get("canonical_target_path"))
        .and_then(Value::as_str)
        .expect("preview target");

    let (apply_code, apply_stdout, apply_stderr) = run_cli(
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
    assert_eq!(apply_code, 0, "stderr: {apply_stderr}");
    let apply: Value = serde_json::from_str(&apply_stdout).expect("apply json");
    let apply_target = apply
        .get("execution")
        .and_then(|value| value.get("canonical_target_path"))
        .and_then(Value::as_str)
        .expect("apply target");
    assert_eq!(preview_target, apply_target);
}

#[test]
fn legacy_pipeline_elimination_warns_on_stale_artifact() {
    let root = temp_workspace("stale_warning");
    write_workspace(&root);
    write_mutation_plan(
        &root.join(".dbm/design/mutation.json"),
        &plan_with_source(Some("src/adapter/mod.rs"), true),
    );

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
    let parsed: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(
        parsed
            .get("execution")
            .and_then(|value| value.get("stale_artifact_detected"))
            .and_then(Value::as_bool),
        Some(true),
        "stdout: {stdout}"
    );
}

#[test]
fn legacy_pipeline_elimination_fallback_only_when_source_path_missing() {
    let root = temp_workspace("fallback_only");
    write_workspace(&root);
    let resolved = resolve_mutation_target_path(&root, &plan_with_source(None, false))
        .expect("resolve")
        .expect("target");
    assert_eq!(resolved, PathBuf::from("src/adapter/mod.rs"));
}

#[test]
fn legacy_pipeline_elimination_preserves_explicit_target_override() {
    let root = temp_workspace("explicit_override");
    write_workspace(&root);
    write_mutation_plan(
        &root.join(".dbm/design/mutation.json"),
        &plan_with_source(Some("src/adapter/mod.rs"), false),
    );

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
    let target = parsed
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|value| value.get("resolved_relative_path"))
        .and_then(Value::as_str)
        .expect("target");
    assert_eq!(target, "tests/fixtures/demo/src/adapter/mod.rs");
}
