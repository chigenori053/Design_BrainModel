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
    for command in ["analyze", "validate", "refactor"] {
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
            assert!(stdout["data_flow"].is_array());
            assert!(stdout["issues"].is_array());
            let first_issue = stdout["issues"].as_array().and_then(|issues| issues.first()).cloned();
            if let Some(issue) = first_issue {
                assert!(issue["id"].is_string());
                assert!(issue["kind"].is_string());
                assert!(issue["severity"].is_string());
                assert!(issue["description"].is_string());
                assert!(issue["evidence"].is_array());
            }
            assert!(stdout.get("plan").is_none());
            assert!(stdout.get("simulation").is_none());
        }
        if command == "refactor" {
            assert!(stdout["plan"]["steps"].is_array());
            assert!(stdout["simulation"]["before"]["cycle_count"].is_number());
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
fn analyze_no_refactor_output() {
    let dir = temp_project_dir("analyze_pure");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path")])
        .output()
        .expect("run analyze");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("Next Action:"));
    assert!(stdout.contains("cli refactor"));
    assert!(!stdout.contains("Introduce Interface"));
    assert!(!stdout.contains("Split Module"));
    assert!(!stdout.contains("Move Dependency"));
    assert!(!stdout.contains("Simulation:"));
}

#[test]
fn analyze_no_action_words() {
    let dir = temp_project_dir("analyze_words");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path")])
        .output()
        .expect("run analyze");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8").to_ascii_lowercase();
    for word in ["should", "fix", "introduce", "split", "move", "optimize", "improve"] {
        assert!(!stdout.contains(word), "unexpected action word: {word}");
    }
}

#[test]
fn analyze_contains_only_diagnostics() {
    let dir = temp_project_dir("analyze_diagnostics");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path")])
        .output()
        .expect("run analyze");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("Structural Issues") || stdout.contains("Semantic Issues") || stdout.contains("Data Flow Issues"));
    assert!(!stdout.contains("should"));
    assert!(!stdout.contains("fix"));
}

#[test]
fn analyze_output_structure() {
    let dir = temp_project_dir("analyze_structure");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path")])
        .output()
        .expect("run analyze");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("Analysis"));
    assert!(stdout.contains("Structural Issues"));
    assert!(stdout.contains("Semantic Issues"));
    assert!(stdout.contains("Data Flow Issues"));
    assert!(stdout.contains("Summary:"));
    assert!(stdout.contains("Next Action:"));
}

#[test]
fn next_action_only_refactor_command() {
    let dir = temp_project_dir("analyze_next_action");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path")])
        .output()
        .expect("run analyze");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains(&format!("cli refactor {}", dir.display())));
    assert!(!stdout.contains("Fix this"));
    assert!(!stdout.contains("fix by"));
}

#[test]
fn cli_analyze_pure_mode() {
    let dir = temp_project_dir("analyze_cli_pure");
    let out = Command::new(cli_bin())
        .args(["analyze", dir.to_str().expect("utf8 path"), "--json"])
        .output()
        .expect("run analyze json");

    assert_eq!(out.status.code(), Some(0));
    let stdout: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
    assert!(stdout["issues"].is_array());
    assert!(stdout["summary"].is_object());
    assert_eq!(stdout["next_action"], format!("cli refactor {}", dir.display()));
    assert!(stdout.get("plan").is_none());
    assert!(stdout.get("simulation").is_none());
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
