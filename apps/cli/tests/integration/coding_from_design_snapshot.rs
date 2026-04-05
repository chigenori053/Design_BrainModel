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
    let dir = std::env::temp_dir().join(format!("design_cli_snapshot_mutation_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("src");
    dir
}

fn write_workspace(root: &Path, include_ports: bool, use_world_call: bool) {
    fs::create_dir_all(root.join("src/adapter")).expect("adapter dir");
    fs::create_dir_all(root.join("src/world")).expect("world dir");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"snapshot_mutation\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    let mut lib_rs = "pub mod adapter;\npub mod world;\n".to_string();
    if include_ports {
        fs::create_dir_all(root.join("src/ports")).expect("ports dir");
        lib_rs.push_str("pub mod ports;\n");
    }
    fs::write(root.join("src/lib.rs"), lib_rs).expect("lib");
    let adapter_body = if use_world_call {
        "use crate::world;\n\npub fn bind() {\n    world::ping();\n}\n"
    } else {
        "use crate::world;\n\npub fn bind() {\n    let _ = 1usize;\n}\n"
    };
    fs::write(root.join("src/adapter/mod.rs"), adapter_body).expect("adapter");
    fs::write(root.join("src/world/mod.rs"), "pub fn ping() {}\n").expect("world");
    if include_ports {
        fs::write(root.join("src/ports/mod.rs"), "pub fn ping() {}\n").expect("ports");
    }
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
    .expect("mutation plan");
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
fn coding_from_design_snapshot_is_deterministic_for_stable_edge() {
    let workspace = temp_workspace("deterministic");
    write_workspace(&workspace, true, false);
    let mutation = workspace.join("mutation.json");
    write_mutation_plan(&mutation);

    let (code1, stdout1, stderr1) = run_cli(
        &workspace,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            "mutation.json",
            "--check",
            "--json",
        ],
    );
    assert_eq!(code1, 0, "stderr: {stderr1}");

    let (code2, stdout2, stderr2) = run_cli(
        &workspace,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            "mutation.json",
            "--check",
            "--json",
        ],
    );
    assert_eq!(code2, 0, "stderr: {stderr2}");

    let left: Value = serde_json::from_str(&stdout1).expect("left json");
    let right: Value = serde_json::from_str(&stdout2).expect("right json");
    assert_eq!(left.get("patches"), right.get("patches"));
    assert_eq!(left.get("changes"), right.get("changes"));
}

#[test]
fn coding_from_design_snapshot_does_not_increase_cycles_after_apply() {
    let workspace = temp_workspace("no_new_cycles");
    write_workspace(&workspace, true, false);
    let mutation = workspace.join("mutation.json");
    write_mutation_plan(&mutation);

    let (before_code, before_stdout, before_stderr) =
        run_cli(&workspace, &["analyze", ".", "--design-json"]);
    assert_eq!(before_code, 0, "stderr: {before_stderr}");
    let before: Value = serde_json::from_str(&before_stdout).expect("before json");
    let before_cycles = before
        .get("cycles")
        .and_then(Value::as_array)
        .map(|cycles| cycles.len())
        .expect("before cycles");

    let (apply_code, apply_stdout, apply_stderr) = run_cli(
        &workspace,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            "mutation.json",
            "--apply",
            "--json",
        ],
    );
    assert_eq!(apply_code, 0, "stderr: {apply_stderr}");
    let apply_report: Value = serde_json::from_str(&apply_stdout).expect("apply json");
    assert_eq!(
        apply_report
            .get("execution")
            .and_then(|value| value.get("applied"))
            .and_then(Value::as_bool),
        Some(true),
        "stdout: {apply_stdout}"
    );

    let (after_code, after_stdout, after_stderr) =
        run_cli(&workspace, &["analyze", ".", "--design-json"]);
    assert_eq!(after_code, 0, "stderr: {after_stderr}");
    let after: Value = serde_json::from_str(&after_stdout).expect("after json");
    let after_cycles = after
        .get("cycles")
        .and_then(Value::as_array)
        .map(|cycles| cycles.len())
        .expect("after cycles");
    assert!(
        after_cycles <= before_cycles,
        "{before_cycles} -> {after_cycles}"
    );
}

#[test]
fn coding_from_design_snapshot_rolls_back_when_apply_fails() {
    let workspace = temp_workspace("rollback");
    write_workspace(&workspace, true, false);
    let mutation = workspace.join("mutation.json");
    write_mutation_plan(&mutation);
    let original = fs::read_to_string(workspace.join("src/adapter/mod.rs")).expect("original");
    let fake_bin = install_fake_cargo_failure(&workspace);
    let path_env = std::env::var("PATH").unwrap_or_default();
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            ".",
            "--from-design-snapshot",
            "mutation.json",
            "--apply",
            "--json",
        ])
        .current_dir(&workspace)
        .env("PATH", format!("{}:{}", fake_bin.display(), path_env))
        .output()
        .expect("run design_cli");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(code, 0, "stderr: {stderr}");

    let report: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(
        report
            .get("execution")
            .and_then(|value| value.get("rolled_back"))
            .and_then(Value::as_bool),
        Some(true),
        "stdout: {stdout}"
    );
    let current = fs::read_to_string(workspace.join("src/adapter/mod.rs")).expect("adapter");
    assert_eq!(current, original);
}
