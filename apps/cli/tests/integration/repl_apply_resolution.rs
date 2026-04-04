use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_repl_apply_{name}_{unique}"));
    fs::create_dir_all(dir.join("crates/execution_stability_core/src/determinism"))
        .expect("determinism dir");
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/execution_stability_core\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        dir.join("crates/execution_stability_core/Cargo.toml"),
        "[package]\nname = \"execution_stability_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("crate cargo");
    fs::write(
        dir.join("crates/execution_stability_core/src/lib.rs"),
        "pub mod world;\npub mod determinism;\n",
    )
    .expect("lib");
    fs::write(
        dir.join("crates/execution_stability_core/src/world.rs"),
        "pub fn world() {}\n",
    )
    .expect("world");
    fs::write(
        dir.join("crates/execution_stability_core/src/determinism/mod.rs"),
        "use crate::world;\npub fn check() {}\n",
    )
    .expect("determinism");
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
          "from": "determinism",
          "to": "world",
          "via": null
        }
      },
      "operations": [
        {
          "UpdateDependency": {
            "from": "determinism",
            "to": "world",
            "via": null
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

fn write_candidate_snapshot(workspace: &std::path::Path) {
    fs::create_dir_all(workspace.join(".dbm/refactor/candidates")).expect("candidate dir");
    let file_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(
            fs::read(workspace.join("crates/execution_stability_core/src/determinism/mod.rs"))
                .expect("read source"),
        );
        format!("{:x}", hasher.finalize())
    };
    fs::write(
        workspace.join(".dbm/refactor/candidates/determinism-candidate.json"),
        format!(
            r#"{{
  "candidate_id": "determinism-candidate",
  "module_id": {{ "crate_name": "execution_stability_core", "module_path": "determinism" }},
  "logical_name": "determinism",
  "kind": "RemoveDependency",
  "operation": "RemoveDependency",
  "title": "candidate",
  "rationale": "integration",
  "confidence_milli": 900,
  "confidence": 0.9,
  "from_node": {{
    "qualified_id": {{ "crate_name": "execution_stability_core", "module_path": "determinism" }},
    "logical_name": "determinism",
    "source_path": "crates/execution_stability_core/src/determinism/mod.rs"
  }},
  "to_node": {{
    "qualified_id": {{ "crate_name": "execution_stability_core", "module_path": "determinism" }},
    "logical_name": "determinism",
    "source_path": "crates/execution_stability_core/src/determinism/mod.rs"
  }},
  "patch_plan": {{ "RemoveDependency": {{ "from": "determinism", "to": "world" }} }},
  "source_path": "crates/execution_stability_core/src/determinism/mod.rs",
  "preview_hash": "sha256:{file_hash}",
  "base_file_hash": "{file_hash}",
  "target_nodes": ["determinism"],
  "target_edges": [],
  "target": {{ "RemoveDependency": {{ "from": "determinism", "to": "world" }} }}
}}"#
        ),
    )
    .expect("candidate");
}

#[test]
fn repl_apply_uses_unified_sandbox_module_resolution() {
    let workspace = temp_workspace("repl");
    write_candidate_snapshot(&workspace);
    let patch_path = workspace.join("patches.json");
    write_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .arg("repl")
        .current_dir(&workspace)
        .env("DBM_VIEWER_SKIP_OPEN", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            format!(
                "このプロジェクト全体を解析して\n/apply . --input {} --candidate determinism-candidate --no-build --json\n/exit\n",
                patch_path.display()
            )
            .as_bytes(),
        )
        .expect("write repl input");

    let out = child.wait_with_output().expect("wait repl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("[planner: nl_v2] 1 steps"), "{stdout}");
    assert!(
        stdout.contains(
            "\"resolved_relative_path\":\"crates/execution_stability_core/src/determinism/mod.rs\""
        ) || stdout.contains(
            "\"resolved_relative_path\": \"crates/execution_stability_core/src/determinism/mod.rs\""
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("/crates/execution_stability_core/src/determinism/mod.rs"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("failed to resolve root module file under"),
        "{stdout}"
    );
}
