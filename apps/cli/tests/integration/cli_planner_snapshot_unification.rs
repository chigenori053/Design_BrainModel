use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(args: &[&str]) -> (i32, String, String) {
    run_in(Path::new(env!("CARGO_MANIFEST_DIR")), args)
}

fn run_in(current_dir: &Path, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("run design_cli");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("design_cli_snapshot_unification_{name}_{unique}"));
    fs::create_dir_all(root.join("src")).expect("create src");
    root
}

fn write_single_patch_workspace(root: &Path) {
    fs::write(root.join("src/app.rs"), "pub fn app() {}\n").expect("write app");
    fs::write(
        root.join("patches.json"),
        r#"{
  "patches": [
    {
      "patch_id": "p1",
      "action": {
        "ExtractComponent": {
          "from": "app",
          "component": "widget"
        }
      },
      "operations": [
        {
          "ExtractComponent": {
            "from": "app",
            "component": "widget"
          }
        }
      ],
      "description": "extract widget"
    }
  ]
}"#,
    )
    .expect("write patches");
}

#[test]
fn coding_json_repl_noop_uses_canonical_empty_patches() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (code, stdout, stderr) = run_in(
        &repo_root,
        &[
            "coding",
            ".",
            "--target",
            "apps/cli/src/repl.rs",
            "--check",
            "--json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let out: Value = serde_json::from_str(&stdout).expect("stdout json");
    assert_eq!(out["patches"], Value::Array(vec![]), "stdout: {stdout}");
    assert_eq!(
        out["changes"]["patches"],
        Value::Array(vec![]),
        "stdout: {stdout}"
    );
    assert_eq!(
        out["changes"]["changes"],
        Value::Array(vec![]),
        "stdout: {stdout}"
    );
    assert_eq!(
        out["execution"]["diff"]["diffs"],
        Value::Array(vec![]),
        "stdout: {stdout}"
    );
}

#[test]
fn coding_json_single_patch_modify_keeps_patch_change_diff_counts_aligned() {
    let root = temp_workspace("single_patch");
    write_single_patch_workspace(&root);
    let patch_path = root.join("patches.json");

    let (code, stdout, stderr) = run(&[
        "coding",
        root.to_str().expect("root utf8"),
        "--input",
        patch_path.to_str().expect("patch utf8"),
        "--check",
        "--no-build",
        "--json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}\nstdout: {stdout}");

    let out: Value = serde_json::from_str(&stdout).expect("stdout json");
    let patches = out["patches"].as_array().expect("top-level patches");
    let change_set_patches = out["changes"]["patches"]
        .as_array()
        .expect("change_set patches");
    let changes = out["changes"]["changes"].as_array().expect("changes");
    let diffs = out["execution"]["diff"]["diffs"].as_array().expect("diffs");
    let total_changes = out["changes"]["summary"]["total_changes"]
        .as_u64()
        .expect("total_changes");

    assert_eq!(patches.len(), 1, "stdout: {stdout}");
    assert_eq!(patches.len(), change_set_patches.len(), "stdout: {stdout}");
    assert_eq!(patches.len(), total_changes as usize, "stdout: {stdout}");
    assert_eq!(patches.len(), changes.len(), "stdout: {stdout}");
    assert_eq!(patches.len(), diffs.len(), "stdout: {stdout}");
}
