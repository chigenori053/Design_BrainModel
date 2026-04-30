use std::fs;
use std::process::Command;

fn run(args: &[&str], cwd: &std::path::Path) -> (bool, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run design_cli");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(out.stderr).expect("utf8 stderr");
    (out.status.success(), stdout, stderr)
}

#[test]
fn run_dsl_executes_logs_and_replays() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "demo patch",
            "pipeline": [
                { "type": "generate_patch" },
                { "type": "validate" },
                { "type": "apply" }
            ]
        }"#,
    )
    .unwrap();

    let (ok, stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("run_started"));
    assert!(stdout.contains("decision_required"));
    assert!(stdout.contains("retry"));
    assert!(stdout.contains("fallback"));
    assert!(stdout.contains("run_completed"));

    let run_log = fs::read_dir(dir.path().join(".dbm").join("runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let run_id = run_log.file_stem().unwrap().to_str().unwrap();
    let log = fs::read_to_string(&run_log).unwrap();
    assert!(log.contains("\"type\":\"event\""));
    assert!(log.contains("\"type\":\"safety_snapshot\""));
    assert!(log.contains("\"type\":\"agent_prompt\""));
    assert!(log.contains("\"type\":\"agent_response_raw\""));
    assert!(log.contains("\"type\":\"agent_response_parsed\""));
    assert!(log.contains("\"type\":\"retry_attempt\""));
    assert!(log.contains("\"type\":\"fallback_triggered\""));
    assert!(log.contains("\"type\":\"decision\""));

    let (replay_ok, replay_stdout, replay_stderr) = run(&["replay", run_id], dir.path());
    assert!(replay_ok, "stderr: {replay_stderr}");
    assert!(replay_stdout.contains("run_started"));
    assert!(replay_stdout.contains("retry"));
    assert!(replay_stdout.contains("fallback"));
    assert!(replay_stdout.contains("run_completed"));
}

#[test]
fn run_dsl_context_is_reflected_in_agent_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "Fix syntax error",
            "context": {
                "file": "src/main.rs",
                "code": "fn main() { println!(\"Hello\") }",
                "validation_error": "missing semicolon at end of statement"
            },
            "pipeline": [
                { "type": "generate_patch" }
            ]
        }"#,
    )
    .unwrap();

    let (ok, _stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(ok, "stderr: {stderr}");

    let run_log = fs::read_dir(dir.path().join(".dbm").join("runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = fs::read_to_string(&run_log).unwrap();
    assert!(log.contains("File: src/main.rs"));
    assert!(log.contains("Code:\\nfn main() { println!(\\\"Hello\\\") }"));
    assert!(log.contains("Validation Error:\\nmissing semicolon at end of statement"));
    assert!(log.contains("\"context\""));
    assert!(log.contains("\"file\":\"src/main.rs\""));
    assert!(log.contains("\"code\":\"fn main() { println!(\\\"Hello\\\") }\""));
    assert!(log.contains("\"validation_error\":\"missing semicolon at end of statement\""));
}

#[test]
fn run_dsl_rejects_empty_context() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "demo patch",
            "context": {},
            "pipeline": [{ "type": "validate" }]
        }"#,
    )
    .unwrap();

    let (ok, _stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(!ok);
    assert!(stderr.contains("ValidationError"));
    assert!(stderr.contains("context requires file or code"));
}

#[test]
fn run_dsl_missing_semicolon_retries_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "Fix syntax error",
            "context": {
                "file": "src/main.rs",
                "code": "fn main() { println!(\"Hello\") }",
                "validation_error": "missing semicolon at end of statement"
            },
            "pipeline": [
                { "type": "generate_patch" },
                { "type": "validate" },
                { "type": "apply" }
            ]
        }"#,
    )
    .unwrap();

    let (ok, stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("retry"));
    assert!(!stdout.contains("fallback"));
    assert!(stdout.contains("run_completed"));

    let run_log = fs::read_dir(dir.path().join(".dbm").join("runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = fs::read_to_string(&run_log).unwrap();
    assert!(log.contains("\"type\":\"validation_error\""));
    assert!(log.contains("\"type\":\"retry_reason\""));
    assert!(log.contains("\"type\":\"fix_attempt\""));
    assert!(log.contains("\"type\":\"protocol_retry\""));
    assert!(log.contains("\"type\":\"protocol_fix_attempt\""));
    assert!(log.contains("JsonParse"));
    assert!(log.contains("missing semicolon at end of statement"));
    assert!(log.contains("insert_missing_semicolon"));
}

#[test]
fn run_dsl_type_mismatch_retries_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "Fix type mismatch",
            "context": {
                "file": "src/main.rs",
                "code": "let count: usize = \"1\";",
                "validation_error": "type mismatch: expected usize, found string"
            },
            "pipeline": [
                { "type": "generate_patch" },
                { "type": "validate" },
                { "type": "apply" }
            ]
        }"#,
    )
    .unwrap();

    let (ok, stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("retry"));
    assert!(!stdout.contains("fallback"));
    assert!(stdout.contains("run_completed"));

    let run_log = fs::read_dir(dir.path().join(".dbm").join("runs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = fs::read_to_string(&run_log).unwrap();
    assert!(log.contains("type mismatch: expected usize, found string"));
    assert!(log.contains("align_expression_type"));
}

#[test]
fn run_dsl_invalid_validation_error_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "Fix unknown issue",
            "context": {
                "file": "src/main.rs",
                "code": "fn main() {}",
                "validation_error": "unclassified external failure"
            },
            "pipeline": [
                { "type": "generate_patch" },
                { "type": "validate" }
            ]
        }"#,
    )
    .unwrap();

    let (ok, stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("fallback"));
    assert!(stdout.contains("run_completed"));
}

#[test]
fn run_dsl_rejects_validation_error_without_code() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "demo patch",
            "context": {
                "file": "src/main.rs",
                "validation_error": "missing semicolon"
            },
            "pipeline": [{ "type": "validate" }]
        }"#,
    )
    .unwrap();

    let (ok, _stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(!ok);
    assert!(stderr.contains("ValidationError"));
    assert!(stderr.contains("validation_error requires code"));
}

#[test]
fn run_dsl_rejects_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path().join("task.json");
    fs::write(
        &task,
        r#"{
            "version": "1.0",
            "task": "demo patch",
            "pipeline": [{ "type": "validate" }],
            "ambiguous": true
        }"#,
    )
    .unwrap();

    let (ok, _stdout, stderr) = run(&["run-dsl", task.to_str().unwrap()], dir.path());
    assert!(!ok);
    assert!(stderr.contains("ValidationError"));
    assert!(stderr.contains("unknown field"));
}
