use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_integration_{name}_{unique}"));
    fs::create_dir_all(dir.join("apps/cli/src")).expect("cli src");
    fs::create_dir_all(dir.join("apps/viewer/src")).expect("viewer src");
    fs::write(
        dir.join("apps/cli/src/app.rs"),
        "use crate::world;\nfn app() {}\n",
    )
    .expect("app");
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

fn write_determinism_patch(path: &std::path::Path) {
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
          "via": "determinism_world_interface"
        }
      },
      "operations": [
        {
          "UpdateDependency": {
            "from": "determinism",
            "to": "world",
            "via": "determinism_world_interface"
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

fn write_compilable_determinism_patch(path: &std::path::Path) {
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

fn write_invalid_determinism_patch(path: &std::path::Path) {
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
          "via": "123bad"
        }
      },
      "operations": [
        {
          "UpdateDependency": {
            "from": "determinism",
            "to": "world",
            "via": "123bad"
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

fn write_candidate_snapshot(
    workspace: &std::path::Path,
    candidate_id: &str,
    crate_name: &str,
    logical_name: &str,
    source_path: &str,
) {
    fs::create_dir_all(workspace.join(".dbm/refactor/candidates")).expect("candidate dir");
    let file_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(fs::read(workspace.join(source_path)).expect("read source"));
        format!("{:x}", hasher.finalize())
    };
    fs::write(
        workspace.join(format!(".dbm/refactor/candidates/{candidate_id}.json")),
        format!(
            r#"{{
  "candidate_id": "{candidate_id}",
  "module_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
  "logical_name": "{logical_name}",
  "kind": "RemoveDependency",
  "operation": "RemoveDependency",
  "title": "candidate",
  "rationale": "integration",
  "confidence_milli": 900,
  "confidence": 0.9,
  "from_node": {{
    "qualified_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
    "logical_name": "{logical_name}",
    "source_path": "{source_path}"
  }},
  "to_node": {{
    "qualified_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
    "logical_name": "{logical_name}",
    "source_path": "{source_path}"
  }},
  "patch_plan": {{ "RemoveDependency": {{ "from": "{logical_name}", "to": "world" }} }},
  "source_path": "{source_path}",
  "preview_hash": "sha256:{file_hash}",
  "base_file_hash": "{file_hash}",
  "target_nodes": ["{logical_name}"],
  "target_edges": [],
  "target": {{ "RemoveDependency": {{ "from": "{logical_name}", "to": "world" }} }}
}}"#
        ),
    )
    .expect("candidate");
}

fn init_git_repo(workspace: &std::path::Path) {
    let status = Command::new("git")
        .args(["init"])
        .current_dir(workspace)
        .status()
        .expect("git init");
    assert!(status.success());
    let status = Command::new("git")
        .args(["config", "user.email", "dbm@example.com"])
        .current_dir(workspace)
        .status()
        .expect("git config email");
    assert!(status.success());
    let status = Command::new("git")
        .args(["config", "user.name", "DBM"])
        .current_dir(workspace)
        .status()
        .expect("git config name");
    assert!(status.success());
    let status = Command::new("git")
        .args(["add", "--", "."])
        .current_dir(workspace)
        .status()
        .expect("git add");
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(workspace)
        .status()
        .expect("git commit");
    assert!(status.success());
}

fn init_git_repo_with_branch(workspace: &std::path::Path, branch: &str) {
    init_git_repo(workspace);
    let status = Command::new("git")
        .args(["checkout", "-b", branch])
        .current_dir(workspace)
        .status()
        .expect("git checkout -b");
    assert!(status.success());
}

fn init_bare_remote(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let bare = std::env::temp_dir().join(format!("design_cli_remote_{name}_{unique}.git"));
    let status = Command::new("git")
        .args(["init", "--bare", bare.to_str().expect("utf8 bare")])
        .status()
        .expect("git init bare");
    assert!(status.success());
    bare
}

fn install_fake_gh(
    name: &str,
    auth_ok: bool,
    pr_list_json: &str,
    pr_create_url: &str,
) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_fake_gh_{name}_{unique}"));
    fs::create_dir_all(&dir).expect("create fake gh dir");
    let script = dir.join("gh");
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  if [ \"{auth_ok}\" = \"true\" ]; then exit 0; else exit 1; fi\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"list\" ]; then\n  printf '%s' '{pr_list_json}'\n  exit 0\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  printf '%s' '{pr_create_url}'\n  exit 0\nfi\nexit 1\n"
    );
    fs::write(&script, body).expect("write fake gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
    }
    script
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

#[test]
fn coding_apply_uses_preview_candidate_snapshot_for_logical_module() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_determinism_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_integration\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(workspace.join("src/main.rs"), "fn main() {}\n").expect("main");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "use crate::world;\npub fn check() {}\n",
    )
    .expect("determinism");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_integration",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--no-build",
            "--json",
        ])
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body =
        fs::read_to_string(workspace.join("src/runtime/determinism.rs")).expect("read target");
    assert!(body.contains("determinism_world_interface"), "{body}");
}

#[test]
fn coding_apply_runs_transactional_cargo_check_and_cleans_sandbox() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_transactional_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_transactional\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_transactional",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
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
    assert!(stdout.contains("\"build_ok\": true"), "{stdout}");
    assert!(stdout.contains("\"cleanup_ok\": true"), "{stdout}");
    assert!(
        !workspace
            .join(".dbm/tmp/apply/determinism-candidate")
            .exists()
    );
    let body =
        fs::read_to_string(workspace.join("src/runtime/determinism.rs")).expect("read target");
    assert!(body.contains("use crate::world;"), "{body}");
}

#[test]
fn coding_apply_keeps_real_workspace_unchanged_when_sandbox_check_fails() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_build_fail_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_build_fail\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_build_fail",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_invalid_determinism_patch(&patch_path);
    let original =
        fs::read_to_string(workspace.join("src/runtime/determinism.rs")).expect("read original");

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
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
    assert!(stdout.contains("\"applied\": false"), "{stdout}");
    assert!(stdout.contains("cargo check failed in sandbox"), "{stdout}");
    assert_eq!(
        fs::read_to_string(workspace.join("src/runtime/determinism.rs")).expect("read after"),
        original
    );
    assert!(
        !workspace
            .join(".dbm/tmp/apply/determinism-candidate")
            .exists()
    );
}

#[test]
fn coding_apply_restricted_commit_excludes_unrelated_dirty_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_commit_exact_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::create_dir_all(workspace.join("apps/viewer_gui/src")).expect("viewer");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_commit_exact\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    fs::write(
        workspace.join("apps/viewer_gui/src/app.rs"),
        "pub fn viewer() {}\n",
    )
    .expect("viewer app");
    init_git_repo(&workspace);
    fs::write(
        workspace.join("apps/viewer_gui/src/app.rs"),
        "pub fn viewer() { println!(\"dirty\"); }\n",
    )
    .expect("dirty viewer");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_commit_exact",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let child = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--json",
            "--confirm-commit",
        ])
        .current_dir(&workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn design_cli");
    let out = child.wait_with_output().expect("wait design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"commit_created\": true"), "{stdout}");
    assert!(stdout.contains("\"commit_hash\": \""), "{stdout}");
    let head = Command::new("git")
        .args(["show", "--name-only", "--pretty=format:", "HEAD"])
        .current_dir(&workspace)
        .output()
        .expect("git show");
    let changed = String::from_utf8_lossy(&head.stdout);
    assert!(
        changed
            .lines()
            .any(|line| line.trim() == "src/runtime/determinism.rs")
    );
    assert!(
        !changed
            .lines()
            .any(|line| line.trim() == "apps/viewer_gui/src/app.rs")
    );
}

#[test]
fn coding_apply_commit_decline_keeps_index_clean() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_commit_decline_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_commit_decline\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo(&workspace);
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_commit_decline",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
        ])
        .current_dir(&workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn design_cli");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"n\n")
        .expect("write input");
    let out = child.wait_with_output().expect("wait design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cached = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&workspace)
        .output()
        .expect("git diff cached");
    assert!(String::from_utf8_lossy(&cached.stdout).trim().is_empty());
}

#[test]
fn coding_apply_restricted_push_succeeds_on_feature_branch() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_push_success_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_push_success\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo_with_branch(&workspace, "dbm/push-success");
    let bare = init_bare_remote("push_success");
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(&workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_push_success",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"push_created\": true"), "{stdout}");
    assert!(stdout.contains("\"remote_name\": \"origin\""), "{stdout}");
    let ls_remote = Command::new("git")
        .args(["ls-remote", "--heads", "origin", "dbm/push-success"])
        .current_dir(&workspace)
        .output()
        .expect("git ls-remote");
    assert!(
        !String::from_utf8_lossy(&ls_remote.stdout).trim().is_empty(),
        "remote branch missing"
    );
}

#[test]
fn coding_apply_restricted_push_rejects_protected_branch() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_push_protected_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_push_protected\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo(&workspace);
    let bare = init_bare_remote("push_protected");
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(&workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_push_protected",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("protected branch"), "{stdout}");
}

#[test]
fn coding_apply_restricted_push_fails_on_dry_run_preflight() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_push_dry_run_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_push_dry_run\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo_with_branch(&workspace, "dbm/push-dry-run-fail");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_push_dry_run",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("DryRunFailed"), "{stdout}");
}

#[test]
fn coding_apply_restricted_pr_create_succeeds_as_draft() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_pr_success_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_pr_success\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo_with_branch(&workspace, "dbm/pr-success");
    let bare = init_bare_remote("pr_success");
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(&workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    let gh = install_fake_gh(
        "pr_success",
        true,
        "[]",
        "https://github.com/org/repo/pull/123",
    );
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_pr_success",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--auto-pr",
            "--confirm-pr",
            "--pr-base",
            "main",
            "--json",
        ])
        .env("DBM_GH_BIN", &gh)
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"pr_created\": true"), "{stdout}");
    assert!(stdout.contains("\"draft\": true"), "{stdout}");
    assert!(
        stdout.contains("https://github.com/org/repo/pull/123"),
        "{stdout}"
    );
}

#[test]
fn coding_apply_restricted_pr_create_aborts_on_auth_failure() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_pr_auth_fail_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_pr_auth_fail\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo_with_branch(&workspace, "dbm/pr-auth-fail");
    let bare = init_bare_remote("pr_auth_fail");
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(&workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    let gh = install_fake_gh(
        "pr_auth_fail",
        false,
        "[]",
        "https://github.com/org/repo/pull/123",
    );
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_pr_auth_fail",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--auto-pr",
            "--confirm-pr",
            "--pr-base",
            "main",
            "--json",
        ])
        .env("DBM_GH_BIN", &gh)
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("GitHub authentication unavailable"),
        "{stdout}"
    );
}

#[test]
fn coding_apply_restricted_pr_create_detects_duplicate() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_pr_duplicate_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_pr_duplicate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        workspace.join("src/main.rs"),
        "mod world;\nmod runtime { pub mod determinism; }\nfn main() { runtime::determinism::check(); }\n",
    )
    .expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    init_git_repo_with_branch(&workspace, "dbm/pr-duplicate");
    let bare = init_bare_remote("pr_duplicate");
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(&workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    let gh = install_fake_gh(
        "pr_duplicate",
        true,
        r#"[{"number":77,"url":"https://github.com/org/repo/pull/77"}]"#,
        "https://github.com/org/repo/pull/123",
    );
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_pr_duplicate",
        "determinism",
        "src/runtime/determinism.rs",
    );
    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--auto-commit",
            "--confirm-commit",
            "--auto-push",
            "--confirm-push",
            "--auto-pr",
            "--confirm-pr",
            "--pr-base",
            "main",
            "--json",
        ])
        .env("DBM_GH_BIN", &gh)
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"duplicate_detected\": true"), "{stdout}");
    assert!(stdout.contains("\"pr_created\": false"), "{stdout}");
}

#[test]
fn coding_apply_resolves_directory_module_to_mod_rs_deterministically() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_directory_module_{unique}"));
    fs::create_dir_all(workspace.join("crates/execution_stability_core/src/determinism"))
        .expect("determinism dir");
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
    fs::write(
        workspace.join("crates/execution_stability_core/src/determinism/mod.rs"),
        "use crate::world;\npub fn check() {}\n",
    )
    .expect("determinism mod");

    let patch_path = workspace.join("patches.json");
    write_compilable_determinism_patch(&patch_path);

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args([
            "coding",
            workspace.to_str().expect("utf8 workspace"),
            "--input",
            patch_path.to_str().expect("utf8 patch"),
            "--apply",
            "--no-build",
            "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "\"resolved_path\":\"crates/execution_stability_core/src/determinism/mod.rs\""
        ) || stdout.contains(
            "\"resolved_path\": \"crates/execution_stability_core/src/determinism/mod.rs\""
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "\"resolved_relative_path\":\"crates/execution_stability_core/src/determinism/mod.rs\""
        ) || stdout.contains(
            "\"resolved_relative_path\": \"crates/execution_stability_core/src/determinism/mod.rs\""
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"resolution_strategy\":\"directory_mod_rs\"")
            || stdout.contains("\"resolution_strategy\": \"directory_mod_rs\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("/crates/execution_stability_core/src/determinism/mod.rs"),
        "{stdout}"
    );
    let body = fs::read_to_string(
        workspace.join("crates/execution_stability_core/src/determinism/mod.rs"),
    )
    .expect("read mod.rs");
    assert!(body.contains("use crate::world;"), "{body}");
}
