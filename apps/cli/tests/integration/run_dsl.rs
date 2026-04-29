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
