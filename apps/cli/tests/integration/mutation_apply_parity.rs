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
    let dir = std::env::temp_dir().join(format!("design_cli_mutation_parity_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/adapter")).expect("src adapter");
    fs::create_dir_all(dir.join("src/world")).expect("src world");
    fs::create_dir_all(dir.join("src/ports")).expect("src ports");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/adapter")).expect("fixture adapter");
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mutation_apply_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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

fn write_mutation_plan(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("design dir");
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

fn install_fake_cargo_failure(workspace: &Path) -> PathBuf {
    let bin_dir = workspace.join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("fake cargo dir");
    let script = bin_dir.join("cargo");
    fs::write(
        &script,
        r#"#!/bin/sh
if [ "$1" = "check" ]; then
  echo 'error: synthetic cargo failure' >&2
  exit 101
fi
if [ "$1" = "metadata" ]; then
  exit 0
fi
exec /usr/bin/false
"#,
    )
    .expect("write fake cargo");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
    }
    bin_dir
}

#[test]
fn mutation_apply_parity_uses_same_target_for_check_and_apply() {
    let root = temp_workspace("same_target");
    write_workspace(&root);
    write_mutation_plan(&root.join(".dbm/design/mutation.json"));

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
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|value| value.get("resolved_relative_path"))
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
    let sandbox_target = apply
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|value| value.get("sandbox_path"))
        .and_then(Value::as_str)
        .expect("sandbox path");

    assert_eq!(preview_target, "src/adapter/mod.rs");
    assert!(
        sandbox_target.ends_with("src/adapter/mod.rs"),
        "{sandbox_target}"
    );
}

#[test]
fn mutation_apply_parity_keeps_production_target_with_fixture_present() {
    let root = temp_workspace("fixture_regression");
    write_workspace(&root);
    write_mutation_plan(&root.join(".dbm/design/mutation.json"));

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
    let target = parsed
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|value| value.get("resolved_relative_path"))
        .and_then(Value::as_str)
        .expect("target");
    assert_eq!(target, "src/adapter/mod.rs", "stdout: {stdout}");
}

#[test]
fn mutation_apply_parity_logs_canonical_target_on_rollback() {
    let root = temp_workspace("rollback_log");
    write_workspace(&root);
    write_mutation_plan(&root.join(".dbm/design/mutation.json"));
    let fake_bin = install_fake_cargo_failure(&root);
    let path_env = std::env::var("PATH").unwrap_or_default();
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--apply",
            "--json",
        ])
        .current_dir(&root)
        .env("PATH", format!("{}:{}", fake_bin.display(), path_env))
        .output()
        .expect("run design_cli");
    assert_eq!(out.status.code().unwrap_or(-1), 0);

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let parsed: Value = serde_json::from_str(&stdout).expect("json");
    let reason = parsed
        .get("execution")
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
        .expect("reason");
    assert!(
        reason.contains("canonical target: src/adapter/mod.rs"),
        "{reason}"
    );
}
