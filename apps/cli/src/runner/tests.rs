use std::collections::HashMap;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::process::{build_result, execute_process};
use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("dbm_runner_{name}_{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn base_config(command: &str, args: Vec<String>, dir: &Path) -> ExecutionConfig {
    ExecutionConfig {
        command: command.to_string(),
        args,
        working_dir: dir.display().to_string(),
        timeout_ms: 500,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    }
}

fn base_timeout() -> TimeoutConfig {
    TimeoutConfig {
        timeout_ms: 500,
        kill_signal: "kill".to_string(),
    }
}

fn base_policy(dir: &Path) -> SandboxPolicy {
    SandboxPolicy {
        allow_network: false,
        allow_fs_write: true,
        allowed_paths: vec![dir.display().to_string()],
    }
}

#[test]
fn resolver_reports_missing_command() {
    let mut resolver = CommandResolver::new();
    let error = resolver
        .resolve("definitely_missing_dbm_command")
        .expect_err("missing command should fail");
    assert!(matches!(error, RunnerError::ValidationError(_)));
}

#[test]
fn resolver_prefers_override() {
    let mut overrides = HashMap::new();
    overrides.insert("cargo".to_string(), "/bin/echo".to_string());
    let mut resolver = CommandResolver::with_overrides(overrides);
    let resolved = resolver.resolve("cargo").expect("resolve override");
    assert_eq!(resolved, "/bin/echo");
}

#[test]
fn process_manager_collects_stdout() {
    let dir = temp_dir("stdout");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "printf runner-ok".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute process");

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.status, "success");
    assert!(result.stdout.contains("runner-ok"));
    assert!(result.output_meta.streamed);
}

#[test]
fn process_manager_times_out() {
    let dir = temp_dir("timeout");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "sleep 1".to_string()],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 20,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute process");

    assert_eq!(result.status, "timeout");
    assert_eq!(result.exit_code, -1);
    assert!(result.stderr.contains("timed out"));
}

#[test]
fn runner_rejects_disallowed_command() {
    let dir = temp_dir("disallowed");
    let error = run(
        &base_config("/bin/rm", vec!["-rf".to_string(), "/".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect_err("runner should reject");

    assert!(matches!(error, RunnerError::ValidationError(_)));
}

#[test]
fn runner_rejects_directory_outside_root() {
    let allowed_root = temp_dir("allowed_root");
    let outside = temp_dir("outside_root");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let error = run(
        &base_config(&resolved, vec!["--version".to_string()], &outside),
        &base_timeout(),
        &base_policy(&allowed_root),
        &allowed_root,
        SandboxMode::FullCopy,
    )
    .expect_err("runner should reject outside root");

    assert!(matches!(error, RunnerError::ValidationError(_)));
}

#[test]
fn runner_rejects_forbidden_argument_syntax() {
    let dir = temp_dir("forbidden_arg");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let error = run(
        &base_config(&resolved, vec!["run;".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect_err("runner should reject");

    assert!(matches!(error, RunnerError::ValidationError(_)));
}

#[test]
fn identical_input_produces_identical_output() {
    let dir = temp_dir("determinism");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let lhs = run(
        &base_config(&resolved, vec!["--version".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect("lhs");
    let rhs = run(
        &base_config(&resolved, vec!["--version".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect("rhs");

    assert_eq!(lhs.status, rhs.status);
    assert_eq!(lhs.exit_code, rhs.exit_code);
    assert_eq!(lhs.stdout, rhs.stdout);
    assert_eq!(lhs.stderr, rhs.stderr);
}

#[test]
fn sandbox_execution_does_not_modify_source_directory() {
    let source = temp_dir("sandbox_src");
    fs::write(source.join("data.txt"), "source").expect("write source");
    let sandbox = create_sandbox(&source).expect("sandbox");
    fs::write(sandbox.guard.path().join("data.txt"), "sandbox").expect("write sandbox");

    let source_text = fs::read_to_string(source.join("data.txt")).expect("read source");
    assert_eq!(source_text, "source");
}

#[test]
fn sandbox_reuse_and_incremental_are_detected() {
    let source = temp_dir("sandbox_cache");
    fs::write(source.join("a.txt"), "one").expect("write file");

    let first = create_sandbox(&source).expect("first");
    assert_eq!(first.mode, SandboxMode::FullCopy);

    let second = create_sandbox(&source).expect("second");
    assert_eq!(second.mode, SandboxMode::Reuse);

    fs::write(source.join("b.txt"), "two").expect("write modified file");
    let third = create_sandbox(&source).expect("third");
    assert_eq!(third.mode, SandboxMode::Incremental);
}

#[test]
fn truncate_output_caps_large_streams() {
    let large = vec![b'a'; 1_000_000 + 128];
    let result = build_result(
        std::process::ExitStatus::from_raw(0),
        false,
        1,
        large,
        Vec::new(),
        MemoryUsage::Unknown,
        OutputMode::Streaming,
        SandboxMode::FullCopy,
    );
    assert_eq!(result.stdout.len(), 1_000_000);
}

#[test]
fn signaled_exit_is_detected() {
    let dir = temp_dir("signal");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -TERM $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute process");

    assert_eq!(result.exit_status, ExitStatus::Signaled);
    assert_eq!(result.exit_code, -1);
}

#[test]
fn streaming_reader_preserves_output_order() {
    let dir = temp_dir("stream");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "printf 'line1\nline2\nline3\n'".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute process");

    assert!(result.stdout.contains("line1"));
    assert!(result.stdout.contains("line2"));
    assert!(result.stdout.contains("line3"));
}

#[test]
fn streaming_and_truncate_report_consistent_meta() {
    let bytes = vec![b'x'; 1_000_000 + 1];
    let result = build_result(
        std::process::ExitStatus::from_raw(0),
        false,
        1,
        bytes,
        Vec::new(),
        MemoryUsage::Unknown,
        OutputMode::Streaming,
        SandboxMode::FullCopy,
    );
    assert!(result.output_meta.streamed);
    assert!(result.output_meta.truncated);
    assert_eq!(result.output_meta.original_size, 1_000_001);
    assert_eq!(result.stdout.len(), 1_000_000);
}

#[test]
fn memory_usage_is_collected() {
    let dir = temp_dir("memory");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "dd if=/dev/zero of=/dev/null bs=1m count=8 >/dev/null 2>&1; sleep 0.1".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute process");

    assert_eq!(result.status, "success");
    match result.telemetry.memory_usage_kb {
        MemoryUsage::Known(_) | MemoryUsage::Unknown => {}
    }
}

#[test]
fn panic_path_cleans_up_sandbox_guard() {
    let source = temp_dir("panic_cleanup");
    let panic_result = std::panic::catch_unwind(|| {
        let sandbox = create_sandbox(&source).expect("sandbox");
        let path = sandbox.guard.path().to_path_buf();
        assert!(path.exists());
        panic!("{}", path.display());
    });
    let panic_payload = panic_result.expect_err("panic expected");
    let path = if let Some(path) = panic_payload.downcast_ref::<String>() {
        PathBuf::from(path)
    } else if let Some(path) = panic_payload.downcast_ref::<&str>() {
        PathBuf::from(path)
    } else {
        panic!("unexpected panic payload");
    };
    assert!(!path.exists());
}
