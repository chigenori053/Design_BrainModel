use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_cli")
}

fn temp_project_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::create_dir_all(dir.join("tests")).expect("create tests");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write manifest");
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("write source");
    dir
}

fn temp_sleep_project_dir(name: &str) -> PathBuf {
    let dir = temp_project_dir(name);
    fs::write(
        dir.join("src/main.rs"),
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(2)); }\n",
    )
    .expect("write sleep source");
    dir
}

#[test]
fn generate_command_outputs_json() {
    let out = Command::new(cli_bin())
        .args([
            "generate",
            "--type",
            "api",
            "--lang",
            "rust",
            "--framework",
            "axum",
            "--json",
        ])
        .output()
        .expect("run cli generate");
    assert_eq!(out.status.code(), Some(0));
    let stdout: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
    assert_eq!(stdout["mode"], "command");
    assert_eq!(stdout["interface_type"], "api");
    assert_eq!(stdout["language"], "rust");
    assert_eq!(stdout["framework"], "axum");
}

#[test]
fn analyze_validate_and_run_commands_output_json() {
    let dir = temp_project_dir("analyze");
    for command in ["analyze", "validate"] {
        let out = Command::new(cli_bin())
            .args([command, dir.to_str().expect("utf8 path"), "--json"])
            .output()
            .expect("run cli command");
        assert_eq!(out.status.code(), Some(0), "failed command: {command}");
        let stdout: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
        assert_eq!(stdout["root"], dir.display().to_string());
        if command == "analyze" {
            assert_eq!(stdout["total_files"], 1);
            assert_eq!(stdout["avg_complexity"], "Low");
            assert!(stdout["modules"].is_array());
            assert!(stdout["dependencies"].is_array());
        }
    }

    let out = Command::new(cli_bin())
        .args(["run", dir.to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run cli run");
    assert_eq!(out.status.code(), Some(0));
    let stdout: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
    assert_eq!(stdout["status"], "success");
    assert_eq!(stdout["exit_code"], 0);
    assert!(stdout["root"].as_str().unwrap_or("").contains("dbm_run_"));
    assert!(stdout["command"].as_str().unwrap_or("").ends_with("cargo"));
    assert_eq!(stdout["sandbox"]["allow_network"], false);
    assert_eq!(stdout["sandbox"]["timed_out"], false);
    assert!(
        stdout["sandbox"]["working_dir"]
            .as_str()
            .unwrap_or("")
            .contains("dbm_run_")
    );
    assert!(
        stdout["telemetry"]["memory_usage_kb"].is_number()
            || stdout["telemetry"]["memory_usage_kb"] == "unknown"
    );
    assert_eq!(stdout["output_meta"]["streamed"], true);
    assert!(stdout["sandbox_mode"].is_string());
}

#[test]
fn wizard_mode_wraps_generate() {
    let mut child = Command::new(cli_bin())
        .args(["wizard", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn wizard");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(b"api\nrust\naxum\n").expect("write stdin");
    }

    let out = child.wait_with_output().expect("wizard output");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let json_start = stdout.find('{').expect("json output");
    let payload: Value = serde_json::from_str(&stdout[json_start..]).expect("json parse");
    assert_eq!(payload["mode"], "wizard");
    assert_eq!(payload["framework"], "axum");
}

#[test]
fn repl_mode_runs_command_and_quits() {
    let dir = temp_project_dir("repl");
    let mut child = Command::new(cli_bin())
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "analyze {}", dir.display()).expect("write analyze");
        writeln!(stdin, "quit").expect("write quit");
    }

    let out = child.wait_with_output().expect("repl output");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("REPL Mode"));
    assert!(stdout.contains("Analysis"));
}

#[test]
fn run_command_reports_timeout() {
    let dir = temp_sleep_project_dir("timeout");
    let out = Command::new(cli_bin())
        .args([
            "run",
            dir.to_str().expect("utf8 path"),
            "--timeout-ms",
            "10",
            "--json",
        ])
        .output()
        .expect("run timeout");

    assert_eq!(out.status.code(), Some(0));
    let stdout: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
    assert_eq!(stdout["status"], "timeout");
    assert_eq!(stdout["exit_code"], -1);
    assert_eq!(stdout["sandbox"]["timed_out"], true);
}

#[test]
fn run_command_rejects_parent_traversal() {
    let dir = temp_project_dir("traversal");
    let parent = dir
        .parent()
        .expect("parent")
        .join("..")
        .join(dir.file_name().expect("file name"));
    let out = Command::new(cli_bin())
        .args(["run", parent.to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run traversal");

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).expect("utf8 stderr");
    assert!(stderr.contains("ValidationError"));
}
