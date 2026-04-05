use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn attach_origin_remote(workspace: &std::path::Path, name: &str) -> std::path::PathBuf {
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
    let status = Command::new("git")
        .args(["remote", "add", "origin", bare.to_str().expect("utf8 bare")])
        .current_dir(workspace)
        .status()
        .expect("git remote add");
    assert!(status.success());
    bare
}

fn install_fake_gh(workspace: &std::path::Path) -> std::path::PathBuf {
    let script = workspace.join("fake-gh.sh");
    fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then\n  exit 0\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"view\" ]; then\n  exit 1\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  echo \"https://github.com/example/repo/pull/1\"\n  exit 0\nfi\nexit 1\n",
    )
    .expect("write fake gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
    }
    script
}

fn install_fake_cargo_failure(workspace: &std::path::Path) -> std::path::PathBuf {
    let bin_dir = workspace.join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("fake cargo dir");
    let script = bin_dir.join("cargo");
    fs::write(
        &script,
        r#"#!/bin/sh
if [ "$1" = "check" ]; then
  echo 'error: failed to get `petgraph` as a dependency of package `coding_apply_build_fail v0.1.0`' >&2
  echo 'Caused by:' >&2
  echo '  download of config.json failed' >&2
  echo 'Caused by:' >&2
  echo '  could not resolve host: index.crates.io' >&2
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
    fs::write(workspace.join("src/main.rs"), "mod world;\nfn main() {}\n").expect("main");
    fs::write(workspace.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
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
        .output()
        .expect("run design_cli");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body =
        fs::read_to_string(workspace.join("src/runtime/determinism.rs")).expect("read target");
    assert!(body.contains("use crate::world;"), "{body}");
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
    let fake_cargo = install_fake_cargo_failure(&workspace);
    let path = format!(
        "{}:{}",
        fake_cargo.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .env("PATH", path)
        .env("HOME", &workspace)
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
    assert!(
        stdout.contains("dependency_unavailable (offline cache miss)"),
        "{stdout}"
    );
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
    assert!(stdout.contains("\"commit_created\": false"), "{stdout}");
    assert!(stdout.contains("\"reason\": \"no_commit_created\""), "{stdout}");
    assert!(stdout.contains("apps/viewer_gui/src/app.rs"), "{stdout}");
    let status = Command::new("git")
        .args(["status", "--short"])
        .current_dir(&workspace)
        .output()
        .expect("git status");
    let changed = String::from_utf8_lossy(&status.stdout);
    assert!(
        changed
            .lines()
            .any(|line| line.trim_end() == " M apps/viewer_gui/src/app.rs"),
        "{changed}"
    );
    assert!(
        !changed
            .lines()
            .any(|line| line.trim_end() == "M  apps/viewer_gui/src/app.rs"),
        "{changed}"
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
fn coding_apply_pushes_and_creates_pr_in_phase2() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_phase1_local_only_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_phase1_local_only\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
    init_git_repo_with_branch(&workspace, "dbm/phase2");
    let _bare = attach_origin_remote(&workspace, "phase2");
    let fake_gh = install_fake_gh(&workspace);
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_phase1_local_only",
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
            "--json",
        ])
        .env("DBM_GH_BIN", fake_gh)
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"push_created\": true"), "{stdout}");
    assert!(stdout.contains("\"pr_created\": true"), "{stdout}");
    let telemetry = fs::read_to_string(workspace.join(".dbm/telemetry/remote_integration.json"))
        .expect("remote telemetry");
    assert!(telemetry.contains("\"push_ok\": true"), "{telemetry}");
    assert!(telemetry.contains("\"pr_created\": true"), "{telemetry}");
}

#[test]
fn coding_apply_blocks_commit_on_overlapping_dirty_target() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_overlap_block_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_overlap_block\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
        "pub fn check() { let _ = 0; }\n",
    )
    .expect("determinism");
    init_git_repo(&workspace);
    fs::write(
        workspace.join("src/runtime/determinism.rs"),
        "pub fn check() { let _ = 1; }\n",
    )
    .expect("manual dirty");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_overlap_block",
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
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("CommitBlocked"), "{stdout}");
    assert!(stdout.contains("\"status\": \"failed\""), "{stdout}");
    assert!(stdout.contains("\"committed\": false"), "{stdout}");
}

#[test]
fn coding_apply_allows_detached_head_and_reports_warning() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_detached_head_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_detached_head\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
    init_git_repo_with_branch(&workspace, "dbm/detached-head");
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&workspace)
        .output()
        .expect("head");
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let checkout = Command::new("git")
        .args(["checkout", &sha])
        .current_dir(&workspace)
        .output()
        .expect("checkout detached");
    assert!(checkout.status.success());
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_detached_head",
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
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("warning: detached HEAD"), "{stdout}");
    assert!(stdout.contains("\"commit_created\": true"), "{stdout}");
}

#[test]
fn coding_apply_persists_local_integration_telemetry() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("design_cli_integration_local_telemetry_{unique}"));
    fs::create_dir_all(workspace.join("src/runtime")).expect("runtime");
    fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coding_apply_local_telemetry\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
    init_git_repo_with_branch(&workspace, "dbm/local-telemetry");
    write_candidate_snapshot(
        &workspace,
        "determinism-candidate",
        "coding_apply_local_telemetry",
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
            "--json",
        ])
        .current_dir(&workspace)
        .output()
        .expect("run design_cli");

    assert!(out.status.success());
    let telemetry_path = workspace.join(".dbm/telemetry/local_integration.json");
    assert!(telemetry_path.exists());
    let telemetry = fs::read_to_string(telemetry_path).expect("telemetry");
    assert!(telemetry.contains("\"commit_created\": true"), "{telemetry}");
    assert!(telemetry.contains("\"confirmation\": true"), "{telemetry}");
    assert!(telemetry.contains("\"files_added\": 1"), "{telemetry}");
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
