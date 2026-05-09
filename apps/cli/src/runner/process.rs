use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus as ProcessExitStatus, Stdio};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

use super::types::{
    CpuReleaseTelemetry, ExecutionConfig, ExecutionResult, ExitStatus, MemoryUsage, OutputMeta,
    OutputMode, RunnerError, SandboxMode, SandboxPolicy, Telemetry, TimeoutConfig,
};

const MAX_OUTPUT: usize = 1_000_000;
const MAX_PARALLEL_EXECUTIONS: usize = 2;
const SPAWN_RETRY_ATTEMPTS: usize = 5;
const SPAWN_RETRY_DELAY_MS: u64 = 25;

pub(crate) fn execute_process(
    config: &ExecutionConfig,
    timeout: &TimeoutConfig,
    policy: &SandboxPolicy,
    sandbox_mode: SandboxMode,
) -> Result<ExecutionResult, RunnerError> {
    let _permit = acquire_execution_permit();
    let start = Instant::now();
    let baseline_threads = current_thread_count();
    let mut command = Command::new(&config.command);
    command
        .args(&config.args)
        .current_dir(&config.working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    // Isolate the child into its own process group (PGID == child PID).
    // This allows killing the entire descendant tree on timeout by sending
    // SIGKILL to the negative PGID (`kill -9 -{pid}`).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    configure_environment(&mut command, config, timeout, policy);

    let mut child = spawn_child_with_retry(command)?;
    let pid = child.id();

    let stdout_reader = child
        .stdout
        .take()
        .ok_or_else(|| RunnerError::ExecutionError("failed to capture stdout pipe".to_string()))?;
    let stderr_reader = child
        .stderr
        .take()
        .ok_or_else(|| RunnerError::ExecutionError("failed to capture stderr pipe".to_string()))?;

    let stdout_handle = spawn_output_reader(stdout_reader, config.output_mode);
    let stderr_handle = spawn_output_reader(stderr_reader, config.output_mode);

    let mut peak_memory_kb = sample_memory_usage_kb(pid);
    let timeout_duration = Duration::from_millis(timeout.timeout_ms.max(1));
    let status = loop {
        match child
            .wait_timeout(Duration::from_millis(25))
            .map_err(|err| {
                RunnerError::ExecutionError(format!("failed to wait for process: {err}"))
            })? {
            Some(status) => break Ok((status, false)),
            None if start.elapsed() >= timeout_duration => {
                child.kill().map_err(|err| {
                    RunnerError::TimeoutError(format!("failed to kill timed out process: {err}"))
                })?;
                cleanup_process_group(pid, "-9");
                let waited = child.wait().map_err(|err| {
                    RunnerError::TimeoutError(format!("failed to reap timed out process: {err}"))
                })?;
                break Ok((waited, true));
            }
            None => {
                peak_memory_kb = merge_memory_usage(peak_memory_kb, sample_memory_usage_kb(pid));
            }
        }
    }?;

    let stdout_bytes = stdout_handle
        .join()
        .map_err(|_| RunnerError::ExecutionError("stdout reader thread panicked".to_string()))?;
    let stderr_bytes = stderr_handle
        .join()
        .map_err(|_| RunnerError::ExecutionError("stderr reader thread panicked".to_string()))?;

    if !status.1 {
        // Even after a normal exit, descendants can remain alive in the child's
        // process group. Best-effort cleanup avoids orphaned grandchildren and
        // keeps repeated runner tests from accumulating stray processes.
        cleanup_process_group(pid, "-15");
        cleanup_process_group(pid, "-9");
    }

    let cpu_release = collect_cpu_release_telemetry(pid, baseline_threads);

    Ok(build_result(
        status.0,
        status.1,
        start.elapsed().as_millis(),
        stdout_bytes,
        stderr_bytes,
        peak_memory_kb,
        cpu_release,
        config.output_mode,
        sandbox_mode,
    ))
}

struct ExecutionPermit;

impl Drop for ExecutionPermit {
    fn drop(&mut self) {
        let (lock, cvar) = execution_gate();
        let mut in_flight = lock.lock().expect("execution gate poisoned");
        *in_flight = in_flight.saturating_sub(1);
        cvar.notify_one();
    }
}

fn execution_gate() -> &'static (Mutex<usize>, Condvar) {
    static GATE: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();
    GATE.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

fn acquire_execution_permit() -> ExecutionPermit {
    let (lock, cvar) = execution_gate();
    let mut in_flight = lock.lock().expect("execution gate poisoned");
    while *in_flight >= MAX_PARALLEL_EXECUTIONS {
        in_flight = cvar.wait(in_flight).expect("execution gate poisoned");
    }
    *in_flight += 1;
    ExecutionPermit
}

fn spawn_child_with_retry(mut command: Command) -> Result<std::process::Child, RunnerError> {
    let mut last_error = None;
    for attempt in 0..SPAWN_RETRY_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(err) if is_retryable_spawn_error(&err) && attempt + 1 < SPAWN_RETRY_ATTEMPTS => {
                last_error = Some(err);
                thread::sleep(Duration::from_millis(
                    SPAWN_RETRY_DELAY_MS * (attempt as u64 + 1),
                ));
            }
            Err(err) => {
                return Err(RunnerError::ExecutionError(format!(
                    "failed to execute process: {err}"
                )));
            }
        }
    }
    let err = last_error.expect("retry loop should retain last error");
    Err(RunnerError::ExecutionError(format!(
        "failed to execute process after retries: {err}"
    )))
}

fn is_retryable_spawn_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::ResourceBusy
    ) || err.raw_os_error() == Some(35)
}

fn cleanup_process_group(pid: u32, signal: &str) {
    #[cfg(unix)]
    {
        let sig = match signal {
            "-15" => 15,
            "-9" => 9,
            _ => return,
        };
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let _ = unsafe { kill(-(pid as i32), sig) };
    }
}

pub(crate) fn build_result(
    status: ProcessExitStatus,
    timed_out: bool,
    duration_ms: u128,
    stdout_bytes: Vec<u8>,
    stderr_bytes: Vec<u8>,
    memory_usage_kb: MemoryUsage,
    cpu_release: CpuReleaseTelemetry,
    output_mode: OutputMode,
    sandbox_mode: SandboxMode,
) -> ExecutionResult {
    let duration_ms = duration_ms.max(1);
    let stdout_meta = OutputMeta {
        streamed: matches!(output_mode, OutputMode::Streaming),
        truncated: stdout_bytes.len() > MAX_OUTPUT,
        original_size: stdout_bytes.len(),
    };
    let stderr_meta = OutputMeta {
        streamed: matches!(output_mode, OutputMode::Streaming),
        truncated: stderr_bytes.len() > MAX_OUTPUT,
        original_size: stderr_bytes.len(),
    };
    let stdout = truncate_output(stdout_bytes);
    let mut stderr = truncate_output(stderr_bytes);

    let exit_status = if timed_out {
        ExitStatus::Signaled
    } else {
        match status.code() {
            Some(code) => ExitStatus::Code(code),
            None => ExitStatus::Signaled,
        }
    };
    let exit_code = match exit_status {
        ExitStatus::Code(code) => code,
        ExitStatus::Signaled => -1,
    };
    let status_text = if timed_out {
        if stderr.is_empty() {
            stderr = "process timed out".to_string();
        } else {
            stderr.push('\n');
            stderr.push_str("process timed out");
        }
        "timeout".to_string()
    } else if exit_code == 0 {
        "success".to_string()
    } else {
        "failure".to_string()
    };

    ExecutionResult {
        status: status_text,
        exit_code,
        exit_status,
        stdout,
        stderr,
        duration_ms,
        telemetry: Telemetry {
            duration_ms,
            exit_code,
            stdout_size: stdout_meta.original_size,
            stderr_size: stderr_meta.original_size,
            memory_usage_kb,
            cpu_release,
        },
        output_meta: stdout_meta,
        stderr_meta,
        sandbox_mode,
    }
}

pub(crate) fn current_thread_count() -> usize {
    let pid = std::process::id().to_string();
    for args in [
        ["-o", "thcount=", "-p", pid.as_str()],
        ["-o", "nlwp=", "-p", pid.as_str()],
    ] {
        let output = match Command::new("ps").args(args).output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        if let Ok(count) = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
        {
            return count;
        }
    }
    0
}

fn collect_cpu_release_telemetry(pid: u32, baseline_threads: usize) -> CpuReleaseTelemetry {
    let cpu_idle_recovery_ms = wait_for_cpu_idle(pid, Duration::from_secs(3));
    let final_threads = current_thread_count();
    let processes_after = process_group_processes(pid);
    let child_processes_after = processes_after
        .iter()
        .filter(|process| process.pid != pid)
        .count();
    let zombie_detected = processes_after
        .iter()
        .any(|process| process.pid != pid && process.stat.contains('Z'));

    CpuReleaseTelemetry {
        baseline_threads,
        final_threads,
        child_processes_after,
        cpu_idle_recovery_ms,
        zombie_detected,
    }
}

fn wait_for_cpu_idle(pid: u32, timeout: Duration) -> u64 {
    let start = Instant::now();
    loop {
        if process_group_processes(pid).is_empty() {
            return start.elapsed().as_millis() as u64;
        }
        if start.elapsed() >= timeout {
            return start.elapsed().as_millis() as u64;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProcessSnapshot {
    pid: u32,
    pgid: u32,
    stat: String,
}

fn process_group_processes(pid: u32) -> Vec<ProcessSnapshot> {
    let pgid = match process_group_id(pid) {
        Some(pgid) => pgid,
        None => return Vec::new(),
    };
    let output = match Command::new("ps")
        .args(["-axo", "pid=,pgid=,stat="])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_process_snapshot)
        .filter(|process| process.pgid == pgid)
        .collect()
}

fn process_group_id(pid: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}

fn parse_process_snapshot(line: &str) -> Option<ProcessSnapshot> {
    let mut parts = line.split_whitespace();
    let pid = parts.next()?.parse::<u32>().ok()?;
    let pgid = parts.next()?.parse::<u32>().ok()?;
    let stat = parts.next()?.to_string();
    Some(ProcessSnapshot { pid, pgid, stat })
}

pub(crate) fn sample_memory_usage_kb(pid: u32) -> MemoryUsage {
    let output = match Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => output,
        Err(_) => return MemoryUsage::Unknown,
    };
    if !output.status.success() {
        return MemoryUsage::Unknown;
    }
    match String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
    {
        Ok(value) => MemoryUsage::Known(value),
        Err(_) => MemoryUsage::Unknown,
    }
}

pub(crate) fn merge_memory_usage(lhs: MemoryUsage, rhs: MemoryUsage) -> MemoryUsage {
    match (lhs, rhs) {
        (MemoryUsage::Known(lhs), MemoryUsage::Known(rhs)) => MemoryUsage::Known(lhs.max(rhs)),
        (MemoryUsage::Known(lhs), MemoryUsage::Unknown) => MemoryUsage::Known(lhs),
        (MemoryUsage::Unknown, MemoryUsage::Known(rhs)) => MemoryUsage::Known(rhs),
        (MemoryUsage::Unknown, MemoryUsage::Unknown) => MemoryUsage::Unknown,
    }
}

fn configure_environment(
    command: &mut Command,
    config: &ExecutionConfig,
    timeout: &TimeoutConfig,
    policy: &SandboxPolicy,
) {
    if config.clean_env {
        command.env_clear();
    }
    for (key, value) in &config.env {
        command.env(key, value);
    }
    command.env("DBM_RUNNER_TIMEOUT_MS", timeout.timeout_ms.to_string());
    command.env(
        "DBM_RUNNER_ALLOW_NETWORK",
        if policy.allow_network { "1" } else { "0" },
    );
    command.env(
        "DBM_RUNNER_ALLOW_FS_WRITE",
        if policy.allow_fs_write { "1" } else { "0" },
    );
    command.env("DBM_RUNNER_WORKDIR", &config.working_dir);

    if !policy.allow_network {
        command.env("CARGO_NET_OFFLINE", "true");
        command.env("PIP_NO_INDEX", "1");
        command.env("NO_PROXY", "*");
        command.env("http_proxy", "");
        command.env("https_proxy", "");
    }
}

fn spawn_output_reader<R>(reader: R, output_mode: OutputMode) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || match output_mode {
        OutputMode::Buffered => {
            let mut bytes = Vec::new();
            let mut reader = BufReader::new(reader);
            let _ = reader.read_to_end(&mut bytes);
            bytes
        }
        OutputMode::Streaming => {
            let mut bytes = Vec::new();
            let mut buffered = BufReader::new(reader);
            let mut line = Vec::new();
            loop {
                line.clear();
                match buffered.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => bytes.extend_from_slice(&line),
                    Err(_) => break,
                }
            }
            bytes
        }
    })
}

fn truncate_output(bytes: Vec<u8>) -> String {
    let mut truncated = bytes;
    if truncated.len() > MAX_OUTPUT {
        truncated.truncate(MAX_OUTPUT);
    }
    String::from_utf8_lossy(&truncated).to_string()
}
