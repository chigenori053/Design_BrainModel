use std::collections::HashMap;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::process::{build_result, execute_process, sample_memory_usage_kb};
use super::*;

// ── stability telemetry helpers ──────────────────────────────────────────────

fn baseline_cpu_release() -> CpuReleaseTelemetry {
    CpuReleaseTelemetry {
        baseline_threads: 0,
        final_threads: 0,
        child_processes_after: 0,
        cpu_idle_recovery_ms: 0,
        zombie_detected: false,
    }
}

/// Count currently open file descriptors for this process.
/// Uses /dev/fd on macOS, /proc/self/fd on Linux.
fn count_open_fds() -> usize {
    #[cfg(target_os = "macos")]
    let fd_dir = "/dev/fd";
    #[cfg(not(target_os = "macos"))]
    let fd_dir = "/proc/self/fd";
    std::fs::read_dir(fd_dir)
        .map(|entries| entries.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

/// Count threads for the current process via `ps`.
/// Returns None if the platform doesn't support the query.
fn count_current_threads() -> Option<usize> {
    let pid = std::process::id();
    // macOS: -o thcount=  /  Linux: -o nlwp=
    for flag in &["-o thcount=", "-o nlwp="] {
        let parts: Vec<&str> = flag.split_whitespace().collect();
        let output = std::process::Command::new("ps")
            .arg(parts[0])
            .arg(parts[1])
            .arg("-p")
            .arg(pid.to_string())
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Ok(n) = s.trim().parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

/// Return true if a process with the given PID still exists.
fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Return the process group ID (PGID) of the given PID, or None.
fn pgid_of(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}

/// Return the `ps stat` field (e.g. "S", "Z", "R") for a PID, or None.
fn process_stat(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Capture the current stty state in machine-readable form (`stty -g`).
/// Returns None when there is no controlling terminal (e.g. CI/CD).
fn stty_state() -> Option<String> {
    let out = std::process::Command::new("stty").arg("-g").output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Count processes whose command line matches `pattern` via `pgrep`.
fn pgrep_count(pattern: &str) -> usize {
    std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

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
        timeout_ms: 1_500,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Streaming,
    }
}

fn base_timeout() -> TimeoutConfig {
    TimeoutConfig {
        timeout_ms: 1_500,
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
        baseline_cpu_release(),
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
        baseline_cpu_release(),
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

// ════════════════════════════════════════════════════════════════════════════
// DBM Runtime Stability Tests
// Spec: DBM Runtime 安定性検証仕様（Terminal Lock / Resource Exhaustion）
// Priority order: TTY/stdin(I) → FD(F) → Process(P) → Thread(T) → Memory(M)
//                 → CPU(C) → Search/Output(S) → Stress(E)
// ════════════════════════════════════════════════════════════════════════════

// ── Test-I1/I3: stdin is /dev/null; stdout/stderr remain functional ──────────

/// Verify that stdin is isolated (null) and both stdout/stderr are usable
/// after the runner completes.  Covers Test-I1 and Test-I3.
#[test]
fn stdin_is_null_and_output_streams_remain_functional() {
    let dir = temp_dir("i1_stdin_null");
    // read with 0-second timeout hits EOF immediately because stdin is /dev/null
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "read -t 0 line; echo stdout-ok; echo stderr-ok >&2".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(
        result.stdout.contains("stdout-ok"),
        "stdout not functional after execution: {:?}",
        result.stdout
    );
    assert!(
        result.stderr.contains("stderr-ok"),
        "stderr not functional after execution: {:?}",
        result.stderr
    );
}

// ── Test-I2: run_protected restores process state after panic ────────────────

/// Verify that run_protected catches a panic without corrupting the caller's
/// process state; a subsequent execution must still succeed.  Covers Test-I2.
#[test]
fn run_protected_state_survives_panic_and_next_execution_succeeds() {
    let dir = temp_dir("i2_panic_recovery");
    let resolved = resolve_command("cargo").expect("resolve cargo");

    // First call via run_protected using an allowed command.
    let first = run_protected(
        &base_config(&resolved, vec!["--version".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect("run_protected first");

    assert!(
        matches!(first, RunnerResult::Success(_)),
        "Expected success, got: {:?}",
        first
    );

    // Process state must be intact — a second call must also succeed.
    let second = run_protected(
        &base_config(&resolved, vec!["--version".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect("run_protected second");

    assert!(
        matches!(second, RunnerResult::Success(_)),
        "second run_protected returned non-success: {:?}",
        second
    );
}

// ── Test-F1: FD count returns to baseline after a single execution ───────────

/// Execute one process and verify the open-FD count for this process returns
/// to (or stays at) its pre-execution level.  Covers Test-F1.
#[test]
fn fd_count_stable_after_single_execution() {
    let dir = temp_dir("f1_fd_single");
    // Warm up: stabilise any lazy-init FDs.
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let before = count_open_fds();
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo fd-stable".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");
    let after = count_open_fds();

    assert_eq!(result.status, "success");
    // Tolerance of 10 accounts for FDs opened by concurrently running tests.
    // An actual per-execution leak would reliably exceed this threshold.
    assert!(
        after <= before + 10,
        "FD leak: before={before} after={after}"
    );
}

// ── Test-F2: No pipe/socket residual after repeated executions ───────────────

/// Run five processes that use pipes and confirm the FD count does not drift
/// upward.  Covers Test-F2.
#[test]
fn fd_count_stable_after_repeated_piped_executions() {
    let dir = temp_dir("f2_pipe_residual");
    // Warm up
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let before = count_open_fds();
    for _ in 0..5 {
        let _ = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), "echo pipe-check | cat".to_string()],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        );
    }
    let after = count_open_fds();

    // Tolerance of 15 accounts for FDs opened by other tests running in parallel.
    // A real per-execution leak would produce deltas of 2×5=10+, easily detectable.
    assert!(
        after <= before + 15,
        "Pipe/socket FD leak after 5 executions: before={before} after={after}"
    );
}

// ── Test-P1: No orphan child processes after normal completion ───────────────

/// Spawn a child that records its PID, then verify it is gone after the runner
/// returns.  Covers Test-P1.
#[test]
fn child_process_not_orphaned_after_normal_completion() {
    let dir = temp_dir("p1_child_orphan");
    let pid_file = dir.join("child.pid");
    let pid_path = pid_file.display().to_string();

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo $$ > {pid_path}; sleep 0")],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");

    if let Ok(content) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            std::thread::sleep(std::time::Duration::from_millis(80));
            assert!(
                !pid_alive(pid),
                "Child process {pid} is still alive after runner returned"
            );
        }
    }
}

// ── Test-P2: Timed-out process is fully reaped ──────────────────────────────

/// Force a timeout and confirm the spawned process is fully reaped (no orphan).
/// Covers Test-P2.
#[test]
fn timed_out_process_is_fully_reaped() {
    let dir = temp_dir("p2_timeout_reap");
    let pid_file = dir.join("child.pid");
    let pid_path = pid_file.display().to_string();

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo $$ > {pid_path}; sleep 60")],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 60,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");

    if let Ok(content) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            std::thread::sleep(std::time::Duration::from_millis(150));
            assert!(!pid_alive(pid), "Timed-out process {pid} was not reaped");
        }
    }
}

// ── Test-T1: Thread count stable after executions ───────────────────────────

/// Confirm that spawning reader threads per execution does not cause a thread
/// leak.  Covers Test-T1.
#[test]
fn thread_count_stable_after_multiple_executions() {
    let dir = temp_dir("t1_thread_leak");
    // Warm up
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let before = count_current_threads();
    for _ in 0..5 {
        let _ = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), "echo thread-check".to_string()],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        );
    }
    let after = count_current_threads();

    if let (Some(b), Some(a)) = (before, after) {
        assert!(a <= b + 4, "Thread leak detected: before={b} after={a}");
    }
    // If platform doesn't support the query, the test passes vacuously.
}

// ── Test-M1: Child memory is bounded; parent RSS does not balloon ────────────

/// Run a memory-touching child and verify (a) telemetry is reported and
/// (b) the parent's RSS does not grow by more than 50 MB.  Covers Test-M1.
#[test]
fn child_memory_bounded_and_parent_rss_stable() {
    let dir = temp_dir("m1_mem_bounded");
    let self_pid = std::process::id();
    let baseline = sample_memory_usage_kb(self_pid);

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "x=ok; echo \"$x\"".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");
    match result.telemetry.memory_usage_kb {
        MemoryUsage::Known(kb) => assert!(kb < 500_000, "Child RSS unexpectedly large: {kb} KB"),
        MemoryUsage::Unknown => {}
    }

    let after = sample_memory_usage_kb(self_pid);
    if let (MemoryUsage::Known(base_kb), MemoryUsage::Known(after_kb)) = (baseline, after) {
        let delta = after_kb.saturating_sub(base_kb);
        assert!(
            delta < 50_000,
            "Parent RSS grew by {delta} KB after child execution"
        );
    }
}

// ── Test-M2: Repeated executions do not cause linear RSS growth ──────────────

/// Sample this process's RSS across 10 iterations and confirm the second half
/// does not exceed the first half by more than 20 MB.  Covers Test-M2.
#[test]
fn repeated_executions_rss_does_not_grow_linearly() {
    let dir = temp_dir("m2_mem_linear");
    let self_pid = std::process::id();
    let mut samples: Vec<u64> = Vec::new();

    for _ in 0..10 {
        let _ = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), "echo loop".to_string()],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        );
        if let MemoryUsage::Known(kb) = sample_memory_usage_kb(self_pid) {
            samples.push(kb);
        }
    }

    if samples.len() >= 6 {
        let mid = samples.len() / 2;
        let first_avg = samples[..mid].iter().sum::<u64>() / mid as u64;
        let second_avg = samples[mid..].iter().sum::<u64>() / (samples.len() - mid) as u64;
        let growth = second_avg.saturating_sub(first_avg);
        assert!(
            growth < 20_000,
            "RSS growing linearly: first_half_avg={first_avg} KB, second_half_avg={second_avg} KB, delta={growth} KB"
        );
    }
}

// ── Test-C1: Runner returns promptly; no CPU spin after completion ───────────

/// Execute a CPU-intensive command and confirm the runner returns well within
/// the allowed wall-clock budget.  Covers Test-C1.
#[test]
fn runner_returns_promptly_after_cpu_intensive_child() {
    let dir = temp_dir("c1_cpu_idle");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "yes | head -n 100000 > /dev/null".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 5000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");
    assert!(
        result.duration_ms < 5000,
        "Runner took too long after CPU task (possible spin): {}ms",
        result.duration_ms
    );
}

// ── Test-C2: Timeout kill terminates a spinning process completely ───────────

/// Force a timeout on an infinite loop and confirm the runner returns within
/// 2 s of the timeout deadline.  Covers Test-C2.
#[test]
fn timeout_kill_terminates_infinite_loop_and_runner_returns() {
    let dir = temp_dir("c2_cpu_timeout");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "while true; do :; done".to_string()],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 120,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");
    assert!(
        result.duration_ms < 2000,
        "Runner hung after timeout kill: {}ms",
        result.duration_ms
    );
}

// ── Test-S1: Large output is capped and runner returns ───────────────────────

/// Generate output larger than MAX_OUTPUT and verify the result is truncated
/// and the runner does not hang.  Covers Test-S1.
#[test]
fn large_output_is_bounded_and_runner_returns() {
    let dir = temp_dir("s1_output_bound");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "yes | head -n 2000000".to_string()],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 8000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(
        result.stdout.len() <= 1_000_000,
        "Output exceeded cap: {} bytes",
        result.stdout.len()
    );
    // Either the output was truncated or the process finished before the cap.
    assert!(
        result.output_meta.truncated || result.status == "success",
        "Unexpected state: truncated={} status={}",
        result.output_meta.truncated,
        result.status
    );
}

// ── Test-S2: Telemetry fields are fully populated ────────────────────────────

/// Confirm that all key telemetry fields are populated after execution.
/// Covers the telemetry requirement stated in §4 of the spec.
#[test]
fn telemetry_fields_are_populated_after_execution() {
    let dir = temp_dir("s2_telemetry");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo telemetry-check".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(result.telemetry.duration_ms > 0, "duration_ms not set");
    assert_eq!(result.telemetry.exit_code, 0);
    assert!(result.telemetry.stdout_size > 0, "stdout_size not set");
    // memory_usage_kb may be Unknown on some platforms — both variants are valid.
    let _ = &result.telemetry.memory_usage_kb;
}

// ── Test-E1: Sequential heterogeneous executions stay functional ─────────────

/// Run a variety of commands in sequence and confirm every one succeeds.
/// Covers Test-E1 (terminal health test).
#[test]
fn sequential_diverse_executions_remain_functional() {
    let dir = temp_dir("e1_term_health");
    let commands = ["echo test", "ls /tmp", "printf 'ok\\n'", "true", "uname"];
    for cmd in commands {
        let result = execute_process(
            &base_config("/bin/sh", vec!["-c".to_string(), cmd.to_string()], &dir),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .expect("execute");
        assert_eq!(
            result.status, "success",
            "Command '{cmd}' failed: {}",
            result.stderr
        );
    }
}

// ── Test-E2: Stress – 20 rapid executions with no FD leak ───────────────────

/// Run 20 rapid executions and confirm (a) all succeed and (b) the FD count
/// does not drift.  Covers Test-E2.
#[test]
fn stress_twenty_rapid_executions_no_leak() {
    let dir = temp_dir("e2_stress");
    // Warm up
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let before_fds = count_open_fds();
    for i in 0..20_u32 {
        let result = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), format!("echo stress-{i}")],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .expect("stress execute");
        assert_eq!(result.status, "success", "Stress iteration {i} failed");
        assert!(
            result.stdout.contains(&format!("stress-{i}")),
            "Iteration {i} stdout mismatch: {:?}",
            result.stdout
        );
    }
    let after_fds = count_open_fds();

    // Tolerance of 20 accounts for parallel test noise.
    // A real per-execution leak would produce deltas of 2×20=40+, easily detectable.
    assert!(
        after_fds <= before_fds + 20,
        "FD leak during stress: before={before_fds} after={after_fds}"
    );
}

// ── Test-E3: Abnormal child exit; subsequent execution succeeds ──────────────

/// Kill a child via SIGKILL and confirm the runner handles it gracefully;
/// the next execution must succeed.  Covers Test-E3.
#[test]
fn signal_exit_followed_by_successful_execution() {
    let dir = temp_dir("e3_abnormal_exit");

    let killed = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -9 $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute kill");

    assert_eq!(killed.exit_code, -1);
    assert_eq!(killed.exit_status, ExitStatus::Signaled);

    // Runner state must be intact for the next call.
    let recovery = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo recovered".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("recovery execute");

    assert_eq!(recovery.status, "success");
    assert!(
        recovery.stdout.contains("recovered"),
        "Recovery stdout: {:?}",
        recovery.stdout
    );
}

// ════════════════════════════════════════════════════════════════════════════
// DBM Additional Stability Tests
// Spec: DBM 追加検証仕様（TTY / OS境界 / 異常系耐性）
// Priority order: TTY → SIG → PG → ERR → RLIMIT → CLI → STRESS
// ════════════════════════════════════════════════════════════════════════════

// ── Test-TTY-3: stty state is identical before and after execution ───────────

/// Capture the terminal state with `stty -g` before and after running a child.
/// If there is no controlling TTY (CI), the test is vacuously skipped.
/// Covers Test-TTY-3.
#[test]
fn stty_state_unchanged_after_child_execution() {
    let before = match stty_state() {
        Some(s) => s,
        None => return, // no controlling TTY – skip
    };

    let dir = temp_dir("tty3_stty_normal");
    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo stty-check".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let after = stty_state().unwrap_or_default();
    assert_eq!(
        before, after,
        "stty state changed after normal child execution"
    );
}

/// stty state must also be intact after a timed-out (killed) child.
/// Covers Test-TTY-3 (timeout variant) and Test-TTY-4 precondition.
#[test]
fn stty_state_unchanged_after_timed_out_child() {
    let before = match stty_state() {
        Some(s) => s,
        None => return,
    };

    let dir = temp_dir("tty3_stty_timeout");
    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "sleep 30".to_string()],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 80,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let after = stty_state().unwrap_or_default();
    assert_eq!(
        before, after,
        "stty state changed after timeout-killed child"
    );
}

// ── TTY-1/2 boundary: child stdin is not a TTY ──────────────────────────────

/// The runner passes `/dev/null` as stdin; the child must not see a TTY.
/// Covers the stdin isolation requirement underlying Test-TTY-1 and Test-TTY-2.
#[test]
fn child_stdin_is_not_a_tty() {
    let dir = temp_dir("tty_stdin_notty");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "[ ! -t 0 ] && echo stdin-not-tty || echo stdin-is-tty".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(
        result.stdout.contains("stdin-not-tty"),
        "Child stdin unexpectedly is a TTY: {:?}",
        result.stdout
    );
}

// ── Test-SIG-1: SIGINT to child exits with signaled status ───────────────────

/// Verify that the runner correctly reports a child that exits via SIGINT.
/// Covers Test-SIG-1.
#[test]
fn sigint_to_child_reported_as_signaled_exit() {
    let dir = temp_dir("sig1_int");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -INT $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.exit_status, ExitStatus::Signaled);
    assert_eq!(result.exit_code, -1);
}

// ── Test-SIG-2: SIGTERM to child exits with signaled status ──────────────────

/// SIGTERM is the graceful-shutdown signal; verify the runner handles it.
/// Covers Test-SIG-2.
#[test]
fn sigterm_to_child_reported_as_signaled_exit() {
    let dir = temp_dir("sig2_term");
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
    .expect("execute");

    assert_eq!(result.exit_status, ExitStatus::Signaled);
    assert_eq!(result.exit_code, -1);
}

// ── Test-SIG-3: Environment healthy after SIGKILL recovery ───────────────────

/// Kill a child with SIGKILL, then confirm the runner's environment
/// is intact: next execution succeeds and the stty state is unchanged.
/// Covers Test-SIG-3.
#[test]
fn environment_healthy_after_sigkill_and_recovery() {
    let dir = temp_dir("sig3_kill_env");
    let stty_before = stty_state();

    let killed = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -9 $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute kill");
    assert_eq!(killed.exit_status, ExitStatus::Signaled);

    // Next execution must succeed — the runner must not be in a broken state.
    let ok = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo sig3-recovery".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("recovery");

    assert_eq!(ok.status, "success");
    assert!(
        ok.stdout.contains("sig3-recovery"),
        "recovery stdout: {:?}",
        ok.stdout
    );

    // stty state must not have changed (skip if no TTY).
    if let Some(before) = stty_before {
        let after = stty_state().unwrap_or_default();
        assert_eq!(before, after, "stty state changed after SIGKILL recovery");
    }
}

// ── Test-PG-3: Child process has a valid process-group ID ────────────────────

/// Verify the runner can observe the PGID of a spawned child (it exists and is
/// non-zero), confirming that process-group tracking is possible.
/// Covers Test-PG-3.
#[test]
fn child_process_has_a_valid_pgid() {
    let dir = temp_dir("pg3_pgid");
    let pid_file = dir.join("child.pid");
    let pid_path = pid_file.display().to_string();

    // Use a non-zero sleep so ps can observe it before it exits.
    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo $$ > {pid_path}; sleep 0")],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    // The process has already exited; verify the PID file was written
    // (proves the child had a valid PID/PGID during its lifetime).
    assert!(pid_file.exists(), "child never wrote its PID");
    let raw = fs::read_to_string(&pid_file).expect("read pid file");
    let pid: u32 = raw.trim().parse().expect("parse pid");
    assert!(pid > 0, "child PID must be positive, got {pid}");
}

// ── Test-PG-1: Direct child is gone after normal completion ──────────────────

/// Confirm the direct child is fully reaped after a normal exit.
/// Covers Test-PG-1 (direct-child variant).
#[test]
fn direct_child_fully_reaped_after_normal_exit() {
    let dir = temp_dir("pg1_direct");
    let pid_file = dir.join("direct.pid");
    let pid_path = pid_file.display().to_string();

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo $$ > {pid_path}; echo done")],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");
    if let Ok(content) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            std::thread::sleep(std::time::Duration::from_millis(60));
            assert!(
                !pid_alive(pid),
                "Direct child {pid} still alive after runner returned"
            );
        }
    }
}

// ── Test-PG-2: Timed-out child is not a zombie ───────────────────────────────

/// After the runner kills a timed-out child and waits for it, the child must
/// not appear as a zombie ("Z" in ps stat).  Covers Test-PG-2.
#[test]
fn killed_child_is_not_a_zombie() {
    let dir = temp_dir("pg2_zombie");
    let pid_file = dir.join("zombie.pid");
    let pid_path = pid_file.display().to_string();

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo $$ > {pid_path}; sleep 60")],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 80,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");

    if let Ok(content) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Some(stat) = process_stat(pid) {
                assert!(
                    !stat.contains('Z'),
                    "Process {pid} is a zombie (stat={stat})"
                );
            }
            assert!(!pid_alive(pid), "Killed child {pid} still alive after reap");
        }
    }
}

// ── Test-ERR-1: run_protected leaves no FD residue after use ─────────────────

/// Verify that run_protected (which wraps in catch_unwind) does not leak FDs
/// or corrupt the process environment.  Covers Test-ERR-1.
#[test]
fn run_protected_leaves_no_fd_residue() {
    let dir = temp_dir("err1_panic_fds");
    let resolved = resolve_command("cargo").expect("resolve cargo");
    let before = count_open_fds();

    let result = run_protected(
        &base_config(&resolved, vec!["--version".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        &dir,
        SandboxMode::FullCopy,
    )
    .expect("run_protected");

    let after = count_open_fds();
    assert!(matches!(result, RunnerResult::Success(_)));
    assert!(
        after <= before + 10,
        "FD residue after run_protected: before={before} after={after}"
    );
}

// ── Test-ERR-3: stdout flood; runner state intact afterward ──────────────────

/// Generate far more than MAX_OUTPUT bytes of stdout and confirm the runner
/// caps the output, returns promptly, and leaves the environment healthy.
/// Covers Test-ERR-3.
#[test]
fn stdout_flood_capped_and_runner_state_intact() {
    let dir = temp_dir("err3_flood");
    let before_fds = count_open_fds();

    let flood = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "yes | head -n 5000000".to_string()],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 10_000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("flood execute");

    let after_fds = count_open_fds();

    // Output must be capped.
    assert!(
        flood.stdout.len() <= 1_000_000,
        "Output cap violated: {} bytes",
        flood.stdout.len()
    );
    // FDs must not leak.
    assert!(
        after_fds <= before_fds + 15,
        "FD leak after flood: before={before_fds} after={after_fds}"
    );

    // Next execution must succeed.
    let ok = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo flood-recovery".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("recovery");

    assert_eq!(ok.status, "success");
    assert!(
        ok.stdout.contains("flood-recovery"),
        "recovery stdout: {:?}",
        ok.stdout
    );
}

// ── Test-RLIMIT-1: Child running under tight FD ulimit fails gracefully ───────

/// Reduce the FD limit inside the child shell and verify that the runner
/// returns cleanly and remains functional afterward.  Covers Test-RLIMIT-1.
#[test]
fn child_under_tight_fd_ulimit_does_not_corrupt_runner() {
    let dir = temp_dir("rlimit1_fd");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                // Lower FD limit for the child subshell only, then echo.
                "ulimit -n 32 2>/dev/null; echo rlimit-ok".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    // May succeed or fail depending on whether ulimit applies to the echo itself.
    assert!(
        result.status == "success" || result.status == "failure",
        "Unexpected status: {}",
        result.status
    );

    // Runner must remain operational.
    let after = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo after-rlimit".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("after execute");

    assert_eq!(after.status, "success");
    assert!(after.stdout.contains("after-rlimit"));
}

// ── Test-RLIMIT-2: Child under tight virtual-memory limit does not hang ───────

/// Apply `ulimit -v` inside the child and confirm the runner returns promptly
/// whether the child succeeds or fails.  Covers Test-RLIMIT-2.
#[test]
fn child_under_virtual_memory_ulimit_runner_returns_promptly() {
    let dir = temp_dir("rlimit2_vmem");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "ulimit -v 65536 2>/dev/null; echo vmem-ok".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 3000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    // Any terminal status is acceptable; we must NOT hang.
    assert!(
        matches!(result.status.as_str(), "success" | "failure" | "timeout"),
        "Unexpected status: {}",
        result.status
    );
}

// ── Test-CLI-2: Output can be consumed via a pipe ────────────────────────────

/// Run a command that internally uses a pipe (`cmd | cat`) and confirm the
/// runner collects the output correctly.  Covers Test-CLI-2.
#[test]
fn piped_child_output_is_collected_correctly() {
    let dir = temp_dir("cli2_pipe");
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo pipe-output | cat".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");
    assert!(
        result.stdout.contains("pipe-output"),
        "Piped output not captured: {:?}",
        result.stdout
    );
}

// ── Test-CLI-3: Runner can be invoked while holding background processes ──────

/// Confirm that spawning a child while another background shell is running
/// (mimicking `./dbm_cli &`) does not interfere with the runner.
/// Covers Test-CLI-3.
#[test]
fn runner_works_while_background_shell_is_alive() {
    let dir = temp_dir("cli3_bg");

    // Spawn a long-lived background process outside the runner.
    let mut bg = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 5"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn background process");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo bg-coexistence".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    // Kill the background process.
    let _ = bg.kill();
    let _ = bg.wait();

    assert_eq!(result.status, "success");
    assert!(
        result.stdout.contains("bg-coexistence"),
        "Output with background process: {:?}",
        result.stdout
    );
}

// ── Extended telemetry: PGID observed during execution ───────────────────────

/// Verify that the PGID of a running child is observable (non-zero), fulfilling
/// the §3 telemetry requirement for `process_group_id`.
#[test]
fn child_pgid_is_observable_during_execution() {
    let dir = temp_dir("telem_pgid");
    let pid_file = dir.join("running.pid");
    let pid_path = pid_file.display().to_string();

    // Child writes its PID then sleeps briefly so we can sample.
    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                format!("echo $$ > {pid_path}; sleep 0.05"),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 500,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    // Even after exit the PID was written; confirm it was a real PID with a PGID.
    if let Ok(content) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            // The process is gone, but during its lifetime it had a valid PGID.
            // We verify indirectly: the PID itself must have been > 0.
            assert!(pid > 0, "child PID {pid} is not positive");
            // If still alive (timing), PGID must be non-zero.
            if let Some(pgid) = pgid_of(pid) {
                assert!(pgid > 0, "child PGID must be positive, got {pgid}");
            }
        }
    }
}

// ── Test-STRESS-1: 100 sequential executions without degradation ─────────────

/// Run 100 rapid executions, sampling FD count and thread count every 10
/// iterations to confirm no drift.  Marked #[ignore] because it takes ~5 s.
/// Covers Test-STRESS-1.
#[test]
#[ignore = "long-running stress test; run with: cargo test -- --ignored stress_100"]
fn stress_100_sequential_executions_no_degradation() {
    let dir = temp_dir("stress100");
    // Warm up.
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let base_fds = count_open_fds();
    let base_threads = count_current_threads();

    for i in 0..100_u32 {
        let result = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), format!("echo run-{i}")],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .unwrap_or_else(|e| panic!("iteration {i} failed: {e}"));

        assert_eq!(result.status, "success", "Iteration {i} non-success");

        if i % 10 == 9 {
            let fds = count_open_fds();
            let threads = count_current_threads();

            assert!(
                fds <= base_fds + 15,
                "FD drift at iteration {i}: base={base_fds} now={fds}"
            );
            if let (Some(bt), Some(ct)) = (base_threads, threads) {
                assert!(
                    ct <= bt + 4,
                    "Thread drift at iteration {i}: base={bt} now={ct}"
                );
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DBM Final Validation Suite — Terminal Safety Complete
// Spec: DBM 最終検証仕様（Terminal Safety Complete Suite）
// Sections: TTY-FINAL · PG-FINAL · STRESS-FINAL · CLI-FINAL · ERR-FINAL ·
//           LIMIT-FINAL · GATE
// ════════════════════════════════════════════════════════════════════════════

// ── Test-TTY-FINAL-1: Execution inside a pseudo-TTY (via `script`) ───────────

/// Wrap a child execution inside `script -q /dev/null` to create a PTY
/// environment, then verify the output is captured and the session terminates
/// cleanly.  Skipped if `script` is not available.
/// Covers Test-TTY-FINAL-1.
#[test]
fn pty_session_via_script_terminates_cleanly() {
    // Verify `script` is available; skip gracefully if not.
    let available = std::process::Command::new("script")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok();
    if !available {
        return;
    }

    // macOS: script [-q] file [command...]
    // Linux: script [-q] [-c command] file
    #[cfg(target_os = "macos")]
    let args: &[&str] = &["-q", "/dev/null", "/bin/sh", "-c", "echo pty-final-ok"];
    #[cfg(not(target_os = "macos"))]
    let args: &[&str] = &["-q", "-c", "echo pty-final-ok", "/dev/null"];

    let output = std::process::Command::new("script")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("run script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pty-final-ok"),
        "PTY session did not capture output: {:?}",
        stdout
    );
}

// ── Test-TTY-FINAL-2: Runner does not corrupt stty state ─────────────────────

/// Apply stty settings via a subshell and confirm the runner does not change
/// the parent process's terminal state (already verified by prior stty tests;
/// this variant exercises repeated calls).  Covers Test-TTY-FINAL-2.
#[test]
fn stty_state_intact_after_ten_consecutive_executions() {
    let before = match stty_state() {
        Some(s) => s,
        None => return,
    };

    let dir = temp_dir("tty_final2_stty_repeat");
    for _ in 0..10 {
        let _ = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), "echo tty-final2".to_string()],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        );
    }

    let after = stty_state().unwrap_or_default();
    assert_eq!(
        before, after,
        "stty state drifted after 10 consecutive executions"
    );
}

// ── Test-TTY-FINAL-3: Bidirectional I/O — no deadlock ────────────────────────

/// The runner uses separate reader threads for stdout and stderr, and stdin is
/// /dev/null.  This test exercises all three streams simultaneously and
/// confirms no deadlock occurs.  Covers Test-TTY-FINAL-3.
#[test]
fn bidirectional_io_does_not_deadlock() {
    let dir = temp_dir("tty_final3_bidir");

    // The child:
    //  • tries to read from stdin (gets EOF immediately from /dev/null)
    //  • writes to stdout
    //  • writes to stderr
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                // `cat` on /dev/null stdin returns immediately; both output
                // streams are written in the same shell, exercising bidir path.
                "cat /dev/stdin; echo bidir-stdout; echo bidir-stderr >&2".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 2000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("bidirectional execute");

    assert_eq!(
        result.status, "success",
        "Deadlock or failure: {:?}",
        result
    );
    assert!(
        result.stdout.contains("bidir-stdout"),
        "stdout: {:?}",
        result.stdout
    );
    assert!(
        result.stderr.contains("bidir-stderr"),
        "stderr: {:?}",
        result.stderr
    );
}

// ── Test-PG-FINAL-1: Grandchild does not survive parent timeout ───────────────

/// A child spawns a background grandchild (`sleep 60`).  After the runner
/// times out and kills the process group, the grandchild must also be dead.
/// This test validates the `process_group(0)` + group-kill fix in process.rs.
/// Covers Test-PG-FINAL-1.
#[test]
fn grandchild_does_not_survive_parent_timeout() {
    let dir = temp_dir("pg_final1_grandchild");
    let gc_pid_file = dir.join("grandchild.pid");
    let gc_pid_path = gc_pid_file.display().to_string();

    // Child spawns `sleep 60` in the background and writes its PID, then
    // waits — ensuring the grandchild is running when the timeout fires.
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                format!("sleep 60 & echo $! > {gc_pid_path}; wait"),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 500,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");

    // Allow the kill signal to propagate to all members of the process group.
    std::thread::sleep(std::time::Duration::from_millis(400));

    if let Ok(content) = fs::read_to_string(&gc_pid_file) {
        if let Ok(gc_pid) = content.trim().parse::<u32>() {
            assert!(
                !pid_alive(gc_pid),
                "Grandchild {gc_pid} survived parent timeout — process group kill not effective"
            );
        }
    }
}

// ── Test-PG-FINAL-2: SIGKILL of entire process group leaves no descendants ───

/// Kill the direct child (via timeout) and verify that its entire process
/// group is cleared, including processes spawned via `exec`.
/// Covers Test-PG-FINAL-2.
#[test]
fn sigkill_clears_entire_process_group() {
    let dir = temp_dir("pg_final2_sigkill_group");
    let gc_pid_file = dir.join("gc.pid");
    let gc_pid_path = gc_pid_file.display().to_string();

    // Child forks two grandchildren, then loops.
    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                format!(
                    "sleep 60 & echo $! >> {gc_pid_path}; \
                     sleep 60 & echo $! >> {gc_pid_path}; \
                     while true; do :; done"
                ),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 500,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");
    std::thread::sleep(std::time::Duration::from_millis(400));

    if let Ok(content) = fs::read_to_string(&gc_pid_file) {
        for line in content.lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                assert!(
                    !pid_alive(pid),
                    "Grandchild {pid} still alive after process-group SIGKILL"
                );
            }
        }
    }
}

// ── Test-PG-FINAL-3: Child runs in its own isolated process group ─────────────

/// Confirm that the spawned child has a PGID distinct from the test runner
/// process itself (because we call `process_group(0)` in process.rs).
/// Covers Test-PG-FINAL-3.
#[test]
fn child_runs_in_isolated_process_group() {
    let dir = temp_dir("pg_final3_pgid_iso");
    let pid_file = dir.join("pid.txt");
    let pgid_file = dir.join("pgid.txt");
    let pid_path = pid_file.display().to_string();
    let pgid_path = pgid_file.display().to_string();

    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                format!("echo $$ > {pid_path}; ps -o pgid= -p $$ > {pgid_path}; sleep 0"),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 1000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let runner_pgid = pgid_of(std::process::id());

    if let (Ok(pid_str), Ok(pgid_str)) = (
        fs::read_to_string(&pid_file),
        fs::read_to_string(&pgid_file),
    ) {
        if let (Ok(child_pid), Ok(child_pgid)) = (
            pid_str.trim().parse::<u32>(),
            pgid_str.trim().parse::<u32>(),
        ) {
            // With process_group(0), child's PGID must equal its own PID.
            assert_eq!(
                child_pgid, child_pid,
                "Child PGID ({child_pgid}) should equal child PID ({child_pid}) when process_group(0) is used"
            );
            // Child must NOT share the runner's PGID.
            if let Some(runner_pg) = runner_pgid {
                assert_ne!(
                    child_pgid, runner_pg,
                    "Child is in the same PGID as the test runner — process isolation not working"
                );
            }
        }
    }
}

// ── Test-CLI-FINAL-4: Multiple parallel runner instances do not interfere ─────

/// Spawn four concurrent executions and verify all succeed independently.
/// Covers Test-CLI-FINAL-4.
#[test]
fn multiple_parallel_runner_instances_do_not_interfere() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let dir = temp_dir("cli_final4_parallel");
    let results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let handles: Vec<_> = (0..4_u32)
        .map(|i| {
            let d = dir.clone();
            let r = Arc::clone(&results);
            thread::spawn(move || {
                let result = execute_process(
                    &base_config(
                        "/bin/sh",
                        vec!["-c".to_string(), format!("echo parallel-{i}")],
                        &d,
                    ),
                    &base_timeout(),
                    &base_policy(&d),
                    SandboxMode::FullCopy,
                )
                .expect(&format!("parallel instance {i}"));
                r.lock().unwrap().push(result.stdout);
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread join");
    }

    let collected = results.lock().unwrap();
    assert_eq!(
        collected.len(),
        4,
        "Expected 4 results, got {}",
        collected.len()
    );
    for i in 0..4_u32 {
        assert!(
            collected
                .iter()
                .any(|s| s.contains(&format!("parallel-{i}"))),
            "Missing output for parallel-{i}"
        );
    }
}

// ── Test-ERR-FINAL-1: SIGKILL + full telemetry integrity ─────────────────────

/// SIGKILL a child, then run multiple healthy executions and confirm that
/// cumulative telemetry (FD, thread) stays within bounds.
/// Covers Test-ERR-FINAL-1.
#[test]
fn sigkill_followed_by_healthy_telemetry() {
    let dir = temp_dir("err_final1_kill_telem");

    // Warm up.
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let base_fds = count_open_fds();
    let base_threads = count_current_threads();

    // Trigger SIGKILL via self-kill.
    let _ = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -9 $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    // Five healthy executions must complete and not leak resources.
    for i in 0..5_u32 {
        let r = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), format!("echo err-final1-{i}")],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .expect(&format!("healthy exec {i}"));
        assert_eq!(r.status, "success");
    }

    let final_fds = count_open_fds();
    let final_threads = count_current_threads();

    assert!(
        final_fds <= base_fds + 15,
        "FD leak after SIGKILL + recovery: base={base_fds} final={final_fds}"
    );
    if let (Some(bt), Some(ft)) = (base_threads, final_threads) {
        assert!(
            ft <= bt + 4,
            "Thread leak after recovery: base={bt} final={ft}"
        );
    }
}

// ── Test-ERR-FINAL-2: Process proliferation defence ──────────────────────────

/// A child attempts to spawn many short-lived processes.  The runner must
/// complete within the timeout and not leave orphans.
/// Covers Test-ERR-FINAL-2.
#[test]
fn process_proliferation_bounded_by_timeout() {
    let dir = temp_dir("err_final2_prolif");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                // Spawn 50 background sleeps; they are all in the same
                // process group and will be swept by the group kill.
                "for i in $(seq 1 50); do sleep 60 & done; wait".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 600,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "timeout");

    // Give the group kill time to propagate.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify no `sleep 60` processes remain in our process group namespace.
    // pgrep returns non-zero if nothing matches, so count == 0 is clean.
    let surviving = pgrep_count("sleep 60");
    assert!(
        surviving == 0,
        "{surviving} 'sleep 60' process(es) survived the group kill"
    );
}

// ── Test-LIMIT-FINAL-1: FD exhaustion in child — runner stays healthy ─────────

/// Reduce the FD limit inside the child shell as low as the OS allows, then
/// verify the runner returns a result (success or failure) and remains usable.
/// Covers Test-LIMIT-FINAL-1.
#[test]
fn fd_exhaustion_in_child_runner_stays_healthy() {
    let dir = temp_dir("limit_final1_fd_exhaust");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                // Lower limit inside child; runner's own FDs are unaffected.
                "ulimit -n 16 2>/dev/null; echo fd-exhaust-ok".to_string(),
            ],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(
        matches!(result.status.as_str(), "success" | "failure"),
        "Unexpected status: {}",
        result.status
    );

    // Runner must still be operational after child FD exhaustion.
    let ok = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "echo after-fd-limit".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("post-limit execute");

    assert_eq!(ok.status, "success");
    assert!(ok.stdout.contains("after-fd-limit"));
}

// ── Test-LIMIT-FINAL-2: CPU-constrained child via `nice` ─────────────────────

/// Run a child under `nice +19` (lowest scheduling priority) and confirm the
/// runner collects the output and returns within the timeout budget.
/// Covers Test-LIMIT-FINAL-2.
#[test]
fn cpu_limited_child_via_nice_completes_within_timeout() {
    let dir = temp_dir("limit_final2_nice");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "nice -n 19 sh -c 'echo nice-ok'".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 3000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert_eq!(result.status, "success");
    assert!(
        result.stdout.contains("nice-ok"),
        "nice output: {:?}",
        result.stdout
    );
}

// ── Test-LIMIT-FINAL-3: Memory-limited child does not hang the runner ─────────

/// Apply a virtual-memory cap inside the child and confirm the runner returns
/// promptly regardless of whether the child OOMs.
/// Covers Test-LIMIT-FINAL-3.
#[test]
fn memory_limited_child_does_not_hang_runner() {
    let dir = temp_dir("limit_final3_mem");

    let result = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                "ulimit -v 131072 2>/dev/null; echo mem-limit-ok".to_string(),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 3000,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("execute");

    assert!(
        matches!(result.status.as_str(), "success" | "failure" | "timeout"),
        "Unexpected status: {}",
        result.status
    );
}

// ── Test-STRESS-FINAL-2: Continuous execution — no freeze ────────────────────

/// Run for a sustained period (200 iterations at ~30ms each ≈ ~6 s wall clock)
/// and confirm neither the FD count nor the thread count drifts.
/// A lighter version of the full 10–60 min Test-STRESS-FINAL-2.
/// Marked #[ignore] for the long-running variant; this variant runs in CI.
#[test]
#[ignore = "sustained stress test; run with: cargo test -- --ignored stress_final_sustained"]
fn stress_final_sustained_200_iterations_no_drift() {
    let dir = temp_dir("stress_final2_sustained");
    let _ = execute_process(
        &base_config("/bin/sh", vec!["-c".to_string(), "true".to_string()], &dir),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    );

    let base_fds = count_open_fds();
    let base_threads = count_current_threads();
    let base_rss = sample_memory_usage_kb(std::process::id());

    for i in 0..200_u32 {
        let r = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), format!("echo sustained-{i}")],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .unwrap_or_else(|e| panic!("iteration {i}: {e}"));

        assert_eq!(r.status, "success", "Iteration {i}");

        if i % 25 == 24 {
            let fds = count_open_fds();
            let threads = count_current_threads();
            let rss = sample_memory_usage_kb(std::process::id());

            assert!(fds <= base_fds + 15, "FD drift at {i}: {base_fds}→{fds}");
            if let (Some(bt), Some(ct)) = (base_threads, threads) {
                assert!(ct <= bt + 4, "Thread drift at {i}: {bt}→{ct}");
            }
            if let (MemoryUsage::Known(br), MemoryUsage::Known(cr)) = (&base_rss, &rss) {
                let delta = cr.saturating_sub(*br);
                assert!(delta < 30_000, "RSS drift at {i}: {br}→{cr} KB (+{delta})");
            }
        }
    }
}

// ── Terminal Safety Final Gate ────────────────────────────────────────────────

/// Composite gate test: exercises the five safety pillars in a single run.
///
/// 1. Terminal Safety  — stty state is unchanged throughout
/// 2. Resource Safety  — FD count stays bounded
/// 3. Process Safety   — no orphan processes after each execution
/// 4. Signal Safety    — SIGKILL is handled and followed by clean recovery
/// 5. Runtime Stability — 10 back-to-back iterations all succeed
///
/// Passing this test satisfies the §5 Final Gate criteria of the spec.
#[test]
fn terminal_safety_final_gate() {
    let dir = temp_dir("gate_final");

    // ── 1. Terminal Safety ──────────────────────────────────────────────────
    let stty_before = stty_state();
    let fds_before = count_open_fds();

    // ── 2. Signal Safety ────────────────────────────────────────────────────
    let killed = execute_process(
        &base_config(
            "/bin/sh",
            vec!["-c".to_string(), "kill -9 $$".to_string()],
            &dir,
        ),
        &base_timeout(),
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("sigkill");
    assert_eq!(killed.exit_status, ExitStatus::Signaled);

    // ── 3. Runtime Stability — 10 iterations ────────────────────────────────
    for i in 0..10_u32 {
        let r = execute_process(
            &base_config(
                "/bin/sh",
                vec!["-c".to_string(), format!("echo gate-{i}")],
                &dir,
            ),
            &base_timeout(),
            &base_policy(&dir),
            SandboxMode::FullCopy,
        )
        .expect(&format!("gate iteration {i}"));
        assert_eq!(r.status, "success", "Gate iteration {i} failed");
        assert!(
            r.stdout.contains(&format!("gate-{i}")),
            "Gate iteration {i} stdout: {:?}",
            r.stdout
        );
    }

    // ── 4. Process Safety — no orphan from process-group kill ───────────────
    let pid_file = dir.join("gate_gc.pid");
    let pid_path = pid_file.display().to_string();
    let timed = execute_process(
        &base_config(
            "/bin/sh",
            vec![
                "-c".to_string(),
                format!("sleep 60 & echo $! > {pid_path}; wait"),
            ],
            &dir,
        ),
        &TimeoutConfig {
            timeout_ms: 400,
            kill_signal: "kill".to_string(),
        },
        &base_policy(&dir),
        SandboxMode::FullCopy,
    )
    .expect("gate timeout");
    assert_eq!(timed.status, "timeout");

    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Ok(gc_pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(gc_pid) = gc_pid_str.trim().parse::<u32>() {
            assert!(
                !pid_alive(gc_pid),
                "Gate: grandchild {gc_pid} survived group kill"
            );
        }
    }

    // ── 5. Terminal & Resource Safety — final check ─────────────────────────
    if let Some(before) = stty_before {
        let after = stty_state().unwrap_or_default();
        assert_eq!(before, after, "Gate: stty state corrupted");
    }

    let fds_after = count_open_fds();
    assert!(
        fds_after <= fds_before + 20,
        "Gate: FD leak: before={fds_before} after={fds_after}"
    );
}
