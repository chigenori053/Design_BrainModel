use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_integration_{name}_{unique}"));
    fs::create_dir_all(dir.join("apps/cli/src")).expect("cli src");
    fs::create_dir_all(dir.join("apps/viewer/src")).expect("viewer src");
    fs::write(dir.join("apps/cli/src/app.rs"), "use crate::world;\nfn app() {}\n").expect("app");
    fs::write(
        dir.join("apps/viewer/src/renderer.rs"),
        "fn renderer() {}\n",
    )
    .expect("renderer");
    dir
}

fn write_patch(path: &std::path::Path) {
    fs::write(
        path,
        r#"{
  "patches": [
    {
      "patch_id": "p1",
      "action": {
        "MoveDependency": {
          "from": "missing_module",
          "to": "world",
          "via": "app_world_interface"
        }
      },
      "operations": [
        {
          "UpdateDependency": {
            "from": "missing_module",
            "to": "world",
            "via": "app_world_interface"
          }
        }
      ],
      "description": "move dependency"
    }
  ]
}"#,
    )
    .expect("write patch");
}

#[test]
fn coding_target_override_routes_apply_to_requested_file() {
    let workspace = temp_workspace("target_override");
    let patch_path = workspace.join("patches.json");
    write_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--target",
            "apps/cli/src/app.rs",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"apps/cli/src/app.rs\""), "{stdout}");
}
