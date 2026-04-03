use design_cli::runner::{
    ExecutionConfig, OutputMode, SandboxMode, SandboxPolicy, TimeoutConfig, resolve_command, run,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_safety_{name}_{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn base_timeout() -> TimeoutConfig {
    TimeoutConfig {
        timeout_ms: 500,
        kill_signal: "kill".to_string(),
    }
}

fn base_policy(path: &std::path::Path) -> SandboxPolicy {
    SandboxPolicy {
        allow_network: false,
        allow_fs_write: false,
        allowed_paths: vec![path.display().to_string()],
    }
}

#[test]
fn forbidden_shell_syntax_is_rejected() {
    let dir = temp_dir("forbidden_shell");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let config = ExecutionConfig {
        command: resolved,
        args: vec!["build && echo nope".to_string()],
        working_dir: dir.display().to_string(),
        timeout_ms: 500,
        env: Vec::new(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    };

    let error = run(
        &config,
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect_err("forbidden shell syntax must fail");
    assert!(error.to_string().contains("forbidden shell syntax"));
}

#[test]
fn parent_path_traversal_is_rejected() {
    let dir = temp_dir("path_traversal");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let config = ExecutionConfig {
        command: resolved,
        args: vec!["../outside".to_string()],
        working_dir: dir.display().to_string(),
        timeout_ms: 500,
        env: Vec::new(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    };

    let error = run(
        &config,
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect_err("path traversal must fail");
    assert!(
        error
            .to_string()
            .contains("forbidden parent path traversal")
    );
}

#[test]
fn workspace_outside_access_is_rejected() {
    let root = temp_dir("workspace_root");
    let outside = temp_dir("workspace_outside");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let config = ExecutionConfig {
        command: resolved,
        args: vec!["--version".to_string()],
        working_dir: outside.display().to_string(),
        timeout_ms: 500,
        env: Vec::new(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    };

    let error = run(
        &config,
        &base_timeout(),
        &base_policy(&root),
        &root,
        SandboxMode::FullCopy,
    )
    .expect_err("outside access must fail");
    assert!(error.to_string().contains("escapes sandbox root"));
}

#[test]
fn force_flag_is_rejected() {
    let dir = temp_dir("force_flag");
    let resolved = resolve_command("git").expect("resolve git");
    let config = ExecutionConfig {
        command: resolved,
        args: vec!["push".to_string(), "--force".to_string()],
        working_dir: dir.display().to_string(),
        timeout_ms: 500,
        env: Vec::new(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    };

    let error = run(
        &config,
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect_err("force flag must fail");
    assert!(error.to_string().contains("forbidden git pattern"));
}
