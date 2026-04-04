use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::coding::{
    resolve_apply_target, resolve_apply_target_from_modules, resolve_apply_target_relative,
    resolve_sandbox_module_file,
};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_contract_{name}_{unique}"));
    fs::create_dir_all(dir.join("apps/cli/src")).expect("cli src");
    fs::create_dir_all(dir.join("apps/viewer/src")).expect("viewer src");
    fs::create_dir_all(dir.join("core/world/src")).expect("world src");
    fs::write(
        dir.join("apps/cli/src/app.rs"),
        "use crate::world;\nfn app() {}\n",
    )
    .expect("app");
    fs::write(
        dir.join("apps/viewer/src/renderer.rs"),
        "use crate::world;\nfn renderer() {}\n",
    )
    .expect("renderer");
    fs::write(dir.join("core/world/src/lib.rs"), "pub fn world() {}\n").expect("world");
    dir
}

fn write_patch(path: &Path, from: &str) {
    let patch = format!(
        r#"{{
  "patches": [
    {{
      "patch_id": "p1",
      "action": {{
        "MoveDependency": {{
          "from": "{from}",
          "to": "world",
          "via": "app_world_interface"
        }}
      }},
      "operations": [
        {{
          "UpdateDependency": {{
            "from": "{from}",
            "to": "world",
            "via": "app_world_interface"
          }}
        }}
      ],
      "description": "move dependency"
    }}
  ]
}}"#
    );
    fs::write(path, patch).expect("write patch");
}

fn run(args: &[&str]) -> (bool, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("run design_cli");
    (
        out.status.success(),
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
    )
}

#[test]
fn coding_apply_path_resolution_uses_workspace_reverse_lookup_and_target_override() {
    let workspace = temp_workspace("target_override");
    let patch_path = workspace.join("patches.json");
    write_patch(&patch_path, "unknown_module");

    let (ok, stdout, stderr) = run(&[
        "coding",
        workspace.to_str().expect("utf8 workspace"),
        "--input",
        patch_path.to_str().expect("utf8 patch"),
        "--target",
        "apps/cli/src/app.rs",
        "--json",
    ]);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("\"apps/cli/src/app.rs\""), "{stdout}");
    assert!(!stdout.contains("\"./src/app.rs\""), "{stdout}");
}

#[test]
fn coding_apply_path_resolution_rejects_invalid_override_without_writing() {
    let workspace = temp_workspace("invalid_override");
    let patch_path = workspace.join("patches.json");
    write_patch(&patch_path, "unknown_module");
    let target = workspace.join("apps/cli/src/app.rs");
    let before = fs::read_to_string(&target).expect("read target");

    let (ok, _stdout, stderr) = run(&[
        "coding",
        workspace.to_str().expect("utf8 workspace"),
        "--input",
        patch_path.to_str().expect("utf8 patch"),
        "--target",
        "apps/cli/src/missing.rs",
        "--apply",
    ]);
    assert!(!ok, "stderr: {stderr}");
    assert!(stderr.contains("target file does not exist"), "{stderr}");
    assert_eq!(fs::read_to_string(&target).expect("read target"), before);
}

#[test]
fn resolve_apply_target_prefers_directory_mod_rs_for_workspace_crates() {
    let workspace = temp_workspace("directory_module");
    fs::create_dir_all(workspace.join("crates/execution_stability_core/src/determinism"))
        .expect("determinism dir");
    fs::write(
        workspace.join("crates/execution_stability_core/src/determinism/mod.rs"),
        "use crate::world;\npub fn check() {}\n",
    )
    .expect("mod.rs");
    fs::write(
        workspace.join("crates/execution_stability_core/src/lib.rs"),
        "pub mod world;\npub mod determinism;\n",
    )
    .expect("lib");
    fs::write(
        workspace.join("crates/execution_stability_core/src/world.rs"),
        "pub fn world() {}\n",
    )
    .expect("world");

    let resolution =
        resolve_apply_target_from_modules(&workspace, "determinism").expect("resolve determinism");
    assert_eq!(resolution.resolution_strategy, "directory_mod_rs");
    assert_eq!(
        resolution.resolved_path,
        std::path::PathBuf::from("crates/execution_stability_core/src/determinism/mod.rs")
    );
    assert_eq!(
        resolve_apply_target(&workspace, "determinism"),
        Some(std::path::PathBuf::from(
            "crates/execution_stability_core/src/determinism/mod.rs"
        ))
    );
    assert_eq!(
        resolve_apply_target_relative(&workspace, "determinism"),
        Some(std::path::PathBuf::from(
            "crates/execution_stability_core/src/determinism/mod.rs"
        ))
    );
}

#[test]
fn resolve_apply_target_preserves_flat_file_behavior_and_unknown_none() {
    let workspace = temp_workspace("flat_file");
    assert_eq!(
        resolve_apply_target(&workspace, "app"),
        Some(std::path::PathBuf::from("apps/cli/src/app.rs"))
    );
    assert!(resolve_apply_target(&workspace, "unknown").is_none());
    assert!(resolve_apply_target_relative(&workspace, "external").is_none());
}

#[test]
fn resolve_sandbox_module_file_joins_workspace_relative_path() {
    let workspace = temp_workspace("sandbox_join");
    fs::create_dir_all(workspace.join("crates/execution_stability_core/src/determinism"))
        .expect("determinism dir");
    fs::write(
        workspace.join("crates/execution_stability_core/src/determinism/mod.rs"),
        "pub fn check() {}\n",
    )
    .expect("mod.rs");
    let sandbox = workspace.join(".dbm/tmp/sandbox");
    fs::create_dir_all(sandbox.join("crates/execution_stability_core/src/determinism"))
        .expect("sandbox determinism dir");
    fs::write(
        sandbox.join("crates/execution_stability_core/src/determinism/mod.rs"),
        "pub fn check() {}\n",
    )
    .expect("sandbox mod.rs");

    let resolved =
        resolve_sandbox_module_file("determinism", &workspace, &sandbox).expect("sandbox resolve");
    assert_eq!(
        resolved,
        sandbox.join("crates/execution_stability_core/src/determinism/mod.rs")
    );
}
