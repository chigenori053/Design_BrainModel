use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

const WATCHER_JOIN_TIMEOUT: Duration = Duration::from_secs(1);
const THREAD_CONVERGENCE_TIMEOUT: Duration = Duration::from_secs(1);
const CHILD_EXIT_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestScopeTelemetry {
    pub thread_count_before: usize,
    pub thread_count_after: usize,
    pub child_count_after: usize,
    pub zombie_count: usize,
    pub idle_recovery_ms: u64,
    pub temp_dir_residue: usize,
    pub fd_count: usize,
}

struct WatcherRegistration {
    shutdown_tx: Option<Sender<()>>,
    handle: JoinHandle<()>,
}

pub struct TestScopeGuard {
    pub start_threads: usize,
    pub child_pids: Vec<u32>,
    pub sandbox_paths: Vec<PathBuf>,
    pub watcher_handles: Vec<JoinHandle<()>>,
    owned_children: Vec<Child>,
    watcher_registrations: Vec<WatcherRegistration>,
    telemetry: TestScopeTelemetry,
    cleaned: bool,
}

impl TestScopeGuard {
    pub fn new() -> Self {
        Self {
            start_threads: current_thread_count(),
            child_pids: Vec::new(),
            sandbox_paths: Vec::new(),
            watcher_handles: Vec::new(),
            owned_children: Vec::new(),
            watcher_registrations: Vec::new(),
            telemetry: TestScopeTelemetry::default(),
            cleaned: false,
        }
    }

    pub fn register_child(&mut self, pid: u32) {
        if !self.child_pids.contains(&pid) {
            self.child_pids.push(pid);
        }
    }

    pub fn register_child_process(&mut self, child: Child) -> u32 {
        let pid = child.id();
        self.register_child(pid);
        self.owned_children.push(child);
        pid
    }

    pub fn register_sandbox(&mut self, path: PathBuf) {
        self.sandbox_paths.push(path);
    }

    pub fn register_watcher(&mut self, handle: JoinHandle<()>) {
        self.watcher_handles.push(handle);
    }

    pub fn register_watcher_with_shutdown(
        &mut self,
        shutdown_tx: Sender<()>,
        handle: JoinHandle<()>,
    ) {
        self.watcher_registrations.push(WatcherRegistration {
            shutdown_tx: Some(shutdown_tx),
            handle,
        });
    }

    pub fn force_cleanup(&mut self) {
        if self.cleaned {
            return;
        }
        self.cleaned = true;

        let cleanup_started = Instant::now();

        for watcher in &self.watcher_registrations {
            if let Some(tx) = &watcher.shutdown_tx {
                let _ = tx.send(());
            }
        }

        for watcher in self.watcher_registrations.drain(..) {
            join_with_timeout(watcher.handle, WATCHER_JOIN_TIMEOUT);
        }

        for handle in self.watcher_handles.drain(..) {
            join_with_timeout(handle, WATCHER_JOIN_TIMEOUT);
        }

        for child in &mut self.owned_children {
            let _ = child.kill();
        }
        for child in &mut self.owned_children {
            let _ = child.wait();
        }
        self.owned_children.clear();

        for pid in &self.child_pids {
            let _ = terminate_pid(*pid, 15);
        }
        for pid in &self.child_pids {
            if pid_alive(*pid) {
                let _ = terminate_pid(*pid, 9);
            }
        }
        let idle_recovery_ms = wait_for_idle(&self.child_pids, self.start_threads, cleanup_started);

        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        for path in &self.sandbox_paths {
            remove_dir_all_retry(path, 3);
        }

        wait_for_thread_convergence(self.start_threads, THREAD_CONVERGENCE_TIMEOUT);

        self.telemetry = TestScopeTelemetry {
            thread_count_before: self.start_threads,
            thread_count_after: current_thread_count(),
            child_count_after: self
                .child_pids
                .iter()
                .filter(|pid| pid_alive(**pid))
                .count(),
            zombie_count: self
                .child_pids
                .iter()
                .filter(|pid| process_stat(**pid).is_some_and(|stat| stat.contains('Z')))
                .count(),
            idle_recovery_ms,
            temp_dir_residue: self
                .sandbox_paths
                .iter()
                .filter(|path| path.exists())
                .count(),
            fd_count: count_open_fds(),
        };
    }

    pub fn telemetry(&self) -> &TestScopeTelemetry {
        &self.telemetry
    }
}

impl Drop for TestScopeGuard {
    fn drop(&mut self) {
        self.force_cleanup();
    }
}

impl Default for TestScopeGuard {
    fn default() -> Self {
        Self::new()
    }
}

pub fn unique_sandbox_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{unique}"))
}

pub fn create_sandbox_project(
    guard: &mut TestScopeGuard,
    prefix: &str,
    files: &[(&str, &str)],
) -> PathBuf {
    let dir = unique_sandbox_dir(prefix);
    fs::create_dir_all(&dir).expect("create temp dir");
    guard.register_sandbox(dir.clone());
    for (relative, contents) in files {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, contents).expect("write fixture file");
    }
    dir
}

pub fn design_cli_command(exe: &str) -> Command {
    Command::new(exe)
}

pub fn run_design_cli(
    guard: &mut TestScopeGuard,
    exe: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Output {
    let mut command = design_cli_command(exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let child = command.spawn().expect("spawn design_cli");
    guard.register_child(child.id());
    child.wait_with_output().expect("wait for design_cli")
}

pub fn run_design_cli_json(
    guard: &mut TestScopeGuard,
    exe: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Value {
    let out = run_design_cli(guard, exe, args, envs);
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("parse json")
}

pub fn assert_scope_recovered(guard: &TestScopeGuard, max_thread_delta: usize) {
    let telemetry = guard.telemetry();
    assert!(
        telemetry.thread_count_after <= telemetry.thread_count_before + max_thread_delta,
        "thread baseline diverged: before={} after={}",
        telemetry.thread_count_before,
        telemetry.thread_count_after
    );
    assert_eq!(
        telemetry.child_count_after, 0,
        "child processes remain after cleanup"
    );
    assert_eq!(telemetry.zombie_count, 0, "zombie process detected");
    assert_eq!(
        telemetry.temp_dir_residue, 0,
        "temp sandbox residue detected"
    );
}

fn join_with_timeout(handle: JoinHandle<()>, timeout: Duration) {
    let start = Instant::now();
    while !handle.is_finished() && start.elapsed() < timeout {
        thread::sleep(Duration::from_millis(10));
    }
    if handle.is_finished() {
        let _ = handle.join();
    }
}

fn remove_dir_all_retry(path: &Path, retries: usize) {
    for attempt in 0..retries {
        if !path.exists() {
            return;
        }
        if fs::remove_dir_all(path).is_ok() {
            return;
        }
        if attempt + 1 < retries {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn wait_for_thread_convergence(target_threads: usize, timeout: Duration) {
    let start = Instant::now();
    while current_thread_count() > target_threads && start.elapsed() < timeout {
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_idle(child_pids: &[u32], target_threads: usize, start: Instant) -> u64 {
    loop {
        let no_children = child_pids.iter().all(|pid| !pid_alive(*pid));
        let threads_ok = current_thread_count() <= target_threads;
        if no_children && threads_ok {
            return start.elapsed().as_millis() as u64;
        }
        if start.elapsed() >= CHILD_EXIT_TIMEOUT {
            return start.elapsed().as_millis() as u64;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn count_open_fds() -> usize {
    #[cfg(target_os = "macos")]
    let fd_dir = "/dev/fd";
    #[cfg(not(target_os = "macos"))]
    let fd_dir = "/proc/self/fd";
    fs::read_dir(fd_dir)
        .map(|entries| entries.filter_map(|entry| entry.ok()).count())
        .unwrap_or(0)
}

fn current_thread_count() -> usize {
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

fn pid_alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn process_stat(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let stat = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stat.is_empty() { None } else { Some(stat) }
}

fn terminate_pid(pid: u32, signal: i32) -> io::Result<()> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let rc = unsafe { kill(pid as i32, signal) };
        if rc == 0 || !pid_alive(pid) {
            return Ok(());
        }
        Err(io::Error::last_os_error())
    }

    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_cleanup_removes_registered_sandbox() {
        let telemetry = {
            let mut guard = TestScopeGuard::new();
            let dir = create_sandbox_project(&mut guard, "dbm_guard_cleanup", &[("tmp.txt", "ok")]);
            assert!(dir.exists());
            guard.force_cleanup();
            guard.telemetry().clone()
        };

        assert_eq!(telemetry.child_count_after, 0);
        assert_eq!(telemetry.temp_dir_residue, 0);
    }
}
