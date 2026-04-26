use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::runtime_context::RuntimeContext;

const FD_THRESHOLD: isize = 1;
const MEMORY_THRESHOLD_BYTES: isize = 1024 * 1024;
const THREAD_CONVERGENCE_TIMEOUT: Duration = Duration::from_secs(1);
const HANDLE_JOIN_TIMEOUT: Duration = Duration::from_secs(1);
const CHILD_EXIT_TIMEOUT: Duration = Duration::from_secs(1);

pub type TraceId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub processes: isize,
    pub zombies: isize,
    pub threads: isize,
    pub fds: isize,
    pub tasks: isize,
    pub watchers: isize,
    pub memory_bytes: isize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            processes: 0,
            zombies: 0,
            threads: 0,
            fds: FD_THRESHOLD,
            tasks: 0,
            watchers: 0,
            memory_bytes: MEMORY_THRESHOLD_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceLeak {
    Tracked(ResourceTrace),
    Untracked(ResourceDelta),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceSnapshot {
    pub processes: usize,
    pub zombies: usize,
    pub threads: usize,
    pub fds: usize,
    pub tasks: usize,
    pub watchers: usize,
    pub memory_bytes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceDelta {
    pub processes: isize,
    pub zombies: isize,
    pub threads: isize,
    pub fds: isize,
    pub tasks: isize,
    pub watchers: isize,
    pub memory_bytes: isize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceTrace {
    pub trace_id: TraceId,
    pub label: String,
    pub spawned_processes: Vec<u32>,
    pub opened_fds: Vec<i32>,
    pub spawned_tasks: Vec<String>,
    pub spawned_watchers: Vec<String>,
    pub sandbox_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct LeakTelemetry {
    pub trace_id: TraceId,
    pub before: ResourceSnapshot,
    pub observed_after: ResourceSnapshot,
    pub final_after_cleanup: ResourceSnapshot,
    pub observed_delta: ResourceDelta,
    pub final_delta: ResourceDelta,
    pub cleanup_applied: bool,
    pub failure_detected: bool,
    pub leaks: Vec<ResourceLeak>,
    pub issues: Vec<String>,
}

impl LeakTelemetry {
    pub fn leak_detected(&self) -> bool {
        !self.issues.is_empty()
    }
}

struct WatcherHandle {
    id: String,
    shutdown_tx: Option<Sender<()>>,
    handle: JoinHandle<()>,
}

struct TaskHandle {
    id: String,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct TraceState {
    trace: ResourceTrace,
    watchers: Vec<WatcherHandle>,
    tasks: Vec<TaskHandle>,
}

#[derive(Default)]
struct Registry {
    next_trace_id: TraceId,
    traces: HashMap<TraceId, TraceState>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            next_trace_id: 1,
            traces: HashMap::new(),
        })
    })
}

fn scope_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

thread_local! {
    static TRACE_STACK: RefCell<Vec<TraceId>> = const { RefCell::new(Vec::new()) };
}

fn push_current_trace(trace_id: TraceId) {
    TRACE_STACK.with(|stack| stack.borrow_mut().push(trace_id));
}

fn pop_current_trace() {
    TRACE_STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

fn current_trace_id() -> Option<TraceId> {
    TRACE_STACK.with(|stack| stack.borrow().last().copied())
}

pub fn capture_snapshot() -> ResourceSnapshot {
    let pid = std::process::id();
    let (processes, zombies) = child_process_snapshot(pid);
    ResourceSnapshot {
        processes,
        zombies,
        threads: current_thread_count(),
        fds: count_open_fds(),
        tasks: current_task_count(pid),
        watchers: active_watcher_count(),
        memory_bytes: current_memory_bytes(pid),
    }
}

pub fn diff(before: ResourceSnapshot, after: ResourceSnapshot) -> ResourceDelta {
    ResourceDelta {
        processes: after.processes as isize - before.processes as isize,
        zombies: after.zombies as isize - before.zombies as isize,
        threads: after.threads as isize - before.threads as isize,
        fds: after.fds as isize - before.fds as isize,
        tasks: after.tasks as isize - before.tasks as isize,
        watchers: after.watchers as isize - before.watchers as isize,
        memory_bytes: after.memory_bytes as isize - before.memory_bytes as isize,
    }
}

pub fn is_leak(delta: &ResourceDelta) -> bool {
    delta.processes > 0
        || delta.zombies > 0
        || delta.fds > FD_THRESHOLD
        || delta.tasks > 0
        || delta.watchers > 0
}

fn memory_exceeds_threshold(delta: &ResourceDelta) -> bool {
    delta.memory_bytes > MEMORY_THRESHOLD_BYTES
}

fn exceeds_threshold(delta: &ResourceDelta, thresholds: &Thresholds) -> bool {
    delta.processes > thresholds.processes
        || delta.zombies > thresholds.zombies
        || delta.threads > thresholds.threads
        || delta.fds > thresholds.fds
        || delta.tasks > thresholds.tasks
        || delta.watchers > thresholds.watchers
        || delta.memory_bytes > thresholds.memory_bytes
}

pub fn validate_untracked_resources(
    before: &ResourceSnapshot,
    after: &ResourceSnapshot,
    thresholds: &Thresholds,
) -> Result<(), ResourceLeak> {
    let delta = diff(*before, *after);
    if exceeds_threshold(&delta, thresholds) {
        return Err(ResourceLeak::Untracked(delta));
    }
    Ok(())
}

pub fn execute_with_resource_check<R>(
    context: &mut RuntimeContext,
    label: &str,
    operation: impl FnOnce(&mut RuntimeContext) -> R,
) -> R {
    let mut scope = ResourceSafetyScope::begin(label);
    context.resource_trace_id = Some(scope.trace_id());
    let result = panic::catch_unwind(AssertUnwindSafe(|| operation(context)));
    let telemetry = scope.finish(result.is_err());
    context.resource_leak_telemetry = Some(telemetry.clone());
    if let Some(leak) = telemetry.leaks.first() {
        match leak {
            ResourceLeak::Untracked(delta) => {
                panic!("Untracked resource leak detected: Δ={delta:?}")
            }
            ResourceLeak::Tracked(trace) => {
                panic!("Tracked resource leak detected: {:?}", trace)
            }
        }
    }
    match result {
        Ok(value) => value,
        Err(panic_payload) => panic::resume_unwind(panic_payload),
    }
}

pub fn run_resource_safe_test<R>(label: &str, body: impl FnOnce() -> R) -> R {
    let mut scope = ResourceSafetyScope::begin(label);
    let result = panic::catch_unwind(AssertUnwindSafe(body));
    let telemetry = scope.finish(result.is_err());
    if let Some(leak) = telemetry.leaks.first() {
        match leak {
            ResourceLeak::Untracked(delta) => {
                panic!("Untracked resource leak detected: Δ={delta:?}")
            }
            ResourceLeak::Tracked(trace) => {
                panic!("Tracked resource leak detected: {:?}", trace)
            }
        }
    }
    match result {
        Ok(value) => value,
        Err(panic_payload) => panic::resume_unwind(panic_payload),
    }
}

pub fn register_current_child(pid: u32) {
    if let Some(trace_id) = current_trace_id() {
        let mut registry = registry().lock().expect("resource registry poisoned");
        if let Some(trace) = registry.traces.get_mut(&trace_id) {
            if !trace.trace.spawned_processes.contains(&pid) {
                trace.trace.spawned_processes.push(pid);
            }
        }
    }
}

pub fn register_current_fd(fd: i32) {
    if let Some(trace_id) = current_trace_id() {
        let mut registry = registry().lock().expect("resource registry poisoned");
        if let Some(trace) = registry.traces.get_mut(&trace_id) {
            if !trace.trace.opened_fds.contains(&fd) {
                trace.trace.opened_fds.push(fd);
            }
        }
    }
}

pub fn register_current_task(handle: JoinHandle<()>, task_id: impl Into<String>) {
    if let Some(trace_id) = current_trace_id() {
        let mut registry = registry().lock().expect("resource registry poisoned");
        if let Some(trace) = registry.traces.get_mut(&trace_id) {
            let id = task_id.into();
            trace.trace.spawned_tasks.push(id.clone());
            trace.tasks.push(TaskHandle { id, handle });
            return;
        }
    }
    let _ = handle.join();
}

pub fn register_current_watcher(
    handle: JoinHandle<()>,
    watcher_id: impl Into<String>,
    shutdown_tx: Option<Sender<()>>,
) {
    if let Some(trace_id) = current_trace_id() {
        let mut registry = registry().lock().expect("resource registry poisoned");
        if let Some(trace) = registry.traces.get_mut(&trace_id) {
            let id = watcher_id.into();
            trace.trace.spawned_watchers.push(id.clone());
            trace.watchers.push(WatcherHandle {
                id,
                shutdown_tx,
                handle,
            });
            return;
        }
    }
    let _ = handle.join();
}

pub fn register_current_sandbox(path: PathBuf) {
    if let Some(trace_id) = current_trace_id() {
        let mut registry = registry().lock().expect("resource registry poisoned");
        if let Some(trace) = registry.traces.get_mut(&trace_id) {
            trace.trace.sandbox_paths.push(path);
        }
    }
}

pub fn force_cleanup(trace: &ResourceTrace) {
    let mut registry = registry().lock().expect("resource registry poisoned");
    let Some(mut state) = registry.traces.remove(&trace.trace_id) else {
        return;
    };
    drop(registry);

    for watcher in &state.watchers {
        if let Some(tx) = &watcher.shutdown_tx {
            let _ = tx.send(());
        }
    }

    for watcher in state.watchers.drain(..) {
        let _ = watcher.id;
        join_with_timeout(watcher.handle, HANDLE_JOIN_TIMEOUT);
    }

    for task in state.tasks.drain(..) {
        let _ = task.id;
        join_with_timeout(task.handle, HANDLE_JOIN_TIMEOUT);
    }

    for pid in &state.trace.spawned_processes {
        let _ = terminate_pid(*pid, 15);
    }
    for pid in &state.trace.spawned_processes {
        if pid_alive(*pid) {
            let _ = terminate_pid(*pid, 9);
        }
        let _ = reap_pid(*pid);
    }

    for fd in &state.trace.opened_fds {
        let _ = close_fd(*fd);
    }

    for path in &state.trace.sandbox_paths {
        remove_dir_all_retry(path, 3);
    }

    wait_for_idle(
        &state.trace.spawned_processes,
        current_thread_count(),
        Instant::now(),
    );
    wait_for_thread_convergence(current_thread_count(), THREAD_CONVERGENCE_TIMEOUT);
}

pub fn trace_snapshot(trace_id: TraceId) -> Option<ResourceTrace> {
    let registry = registry().lock().expect("resource registry poisoned");
    registry
        .traces
        .get(&trace_id)
        .map(|state| state.trace.clone())
}

struct ResourceSafetyScope {
    _gate: Option<MutexGuard<'static, ()>>,
    trace_id: TraceId,
    before: ResourceSnapshot,
    label: String,
}

impl ResourceSafetyScope {
    fn begin(label: &str) -> Self {
        let gate = if current_trace_id().is_some() {
            None
        } else {
            Some(
                scope_gate()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            )
        };
        let mut registry = registry().lock().expect("resource registry poisoned");
        let trace_id = registry.next_trace_id;
        registry.next_trace_id = registry.next_trace_id.saturating_add(1);
        registry.traces.insert(
            trace_id,
            TraceState {
                trace: ResourceTrace {
                    trace_id,
                    label: label.to_string(),
                    ..ResourceTrace::default()
                },
                ..TraceState::default()
            },
        );
        drop(registry);
        push_current_trace(trace_id);
        Self {
            _gate: gate,
            trace_id,
            before: capture_snapshot(),
            label: label.to_string(),
        }
    }

    fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    fn finish(&mut self, failure_detected: bool) -> LeakTelemetry {
        let observed_after = capture_snapshot();
        let trace = trace_snapshot(self.trace_id).unwrap_or_default();
        force_cleanup(&trace);
        thread::sleep(Duration::from_millis(25));
        let final_after_cleanup = capture_snapshot();
        thread::sleep(Duration::from_millis(25));
        let stabilized_after_cleanup = capture_snapshot();
        pop_current_trace();

        let observed_delta = diff(self.before, observed_after);
        let final_delta = diff(self.before, final_after_cleanup);
        let stabilized_delta = diff(self.before, stabilized_after_cleanup);
        let thresholds = Thresholds::default();
        let mut leaks = Vec::new();
        let mut issues = Vec::new();
        if let Err(leak) =
            validate_untracked_resources(&self.before, &final_after_cleanup, &thresholds)
        {
            issues.push(format!(
                "{} untracked leak after cleanup: {:?}",
                self.label, final_delta
            ));
            leaks.push(leak);
        }
        if let Err(leak) =
            validate_untracked_resources(&self.before, &stabilized_after_cleanup, &thresholds)
        {
            issues.push(format!(
                "{} untracked leak after stabilization: {:?}",
                self.label, stabilized_delta
            ));
            leaks.push(leak);
        }
        if !failure_detected && is_leak(&observed_delta) {
            issues.push(format!(
                "{} leaked before cleanup: {:?}",
                self.label, observed_delta
            ));
        }
        if !failure_detected && memory_exceeds_threshold(&observed_delta) {
            issues.push(format!(
                "{} memory drift exceeded threshold: {}",
                self.label, observed_delta.memory_bytes
            ));
        }
        if !failure_detected
            && (!trace.spawned_processes.is_empty()
                || !trace.opened_fds.is_empty()
                || !trace.spawned_tasks.is_empty()
                || !trace.spawned_watchers.is_empty()
                || !trace.sandbox_paths.is_empty())
        {
            issues.push(format!(
                "{} retained trace resources: {:?}",
                self.label, trace
            ));
            leaks.push(ResourceLeak::Tracked(trace.clone()));
        }
        LeakTelemetry {
            trace_id: self.trace_id,
            before: self.before,
            observed_after,
            final_after_cleanup,
            observed_delta,
            final_delta,
            cleanup_applied: true,
            failure_detected,
            leaks,
            issues,
        }
    }
}

fn active_watcher_count() -> usize {
    let registry = registry().lock().expect("resource registry poisoned");
    registry
        .traces
        .values()
        .map(|state| state.watchers.len())
        .sum()
}

fn count_open_fds() -> usize {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn fcntl(fd: i32, cmd: i32) -> i32;
            fn getdtablesize() -> i32;
        }
        const F_GETFD: i32 = 1;
        let max_fd = unsafe { getdtablesize() }.max(0);
        let mut open = 0usize;
        for fd in 0..max_fd {
            if unsafe { fcntl(fd, F_GETFD) } != -1 {
                open += 1;
            }
        }
        return open;
    }

    #[cfg(not(unix))]
    {
        0
    }
}

fn current_thread_count() -> usize {
    current_task_count(std::process::id())
}

fn current_task_count(pid: u32) -> usize {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{pid}/task");
        if let Ok(entries) = fs::read_dir(path) {
            return entries.filter_map(|entry| entry.ok()).count();
        }
    }

    #[cfg(target_os = "macos")]
    {
        type KernReturn = i32;
        type MachPort = u32;
        type VmAddress = usize;
        type VmSize = usize;
        type MachMsgTypeNumber = u32;
        type ThreadAct = MachPort;
        type ThreadActArray = *mut ThreadAct;

        unsafe extern "C" {
            fn mach_task_self() -> MachPort;
            fn task_threads(
                target_task: MachPort,
                act_list: *mut ThreadActArray,
                act_list_cnt: *mut MachMsgTypeNumber,
            ) -> KernReturn;
            fn vm_deallocate(target_task: MachPort, address: VmAddress, size: VmSize)
            -> KernReturn;
        }

        let mut threads: ThreadActArray = std::ptr::null_mut();
        let mut count: MachMsgTypeNumber = 0;
        let task = unsafe { mach_task_self() };
        let result = unsafe { task_threads(task, &mut threads, &mut count) };
        if result == 0 {
            let _ = unsafe {
                vm_deallocate(
                    task,
                    threads as VmAddress,
                    (count as usize) * std::mem::size_of::<ThreadAct>(),
                )
            };
            return count as usize;
        }
    }

    let _ = pid;
    0
}

#[cfg(target_os = "macos")]
fn child_process_snapshot(pid: u32) -> (usize, usize) {
    unsafe extern "C" {
        fn proc_listchildpids(ppid: i32, buffer: *mut std::ffi::c_void, buffersize: i32) -> i32;
    }

    let mut children = vec![0u32; 256];
    let bytes = unsafe {
        proc_listchildpids(
            pid as i32,
            children.as_mut_ptr().cast(),
            (children.len() * std::mem::size_of::<u32>()) as i32,
        )
    };
    if bytes > 0 {
        let count = if (bytes as usize) <= children.len() {
            bytes as usize
        } else {
            (bytes as usize) / std::mem::size_of::<u32>()
        };
        return (count, 0);
    }
    (0, 0)
}

#[cfg(not(target_os = "macos"))]
fn child_process_snapshot(pid: u32) -> (usize, usize) {
    let output = match Command::new("ps")
        .args(["-axo", "ppid=,pid=,stat="])
        .output()
    {
        Ok(output) => output,
        Err(_) => return (0, 0),
    };
    if !output.status.success() {
        return (0, 0);
    }
    let mut processes = 0;
    let mut zombies = 0;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let Some(ppid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let _pid = fields.next();
        let stat = fields.next().unwrap_or_default();
        if ppid == pid {
            processes += 1;
            if stat.contains('Z') {
                zombies += 1;
            }
        }
    }
    (processes, zombies)
}

fn current_memory_bytes(pid: u32) -> usize {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{pid}/status");
        if let Ok(status) = fs::read_to_string(path) {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    if let Some(kb) = rest
                        .split_whitespace()
                        .find_map(|value| value.parse::<usize>().ok())
                    {
                        return kb.saturating_mul(1024);
                    }
                }
            }
        }
    }

    let output = match Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => output,
        Err(_) => return 0,
    };
    if !output.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<usize>()
        .map(|kb| kb.saturating_mul(1024))
        .unwrap_or(0)
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

fn wait_for_thread_convergence(target_threads: usize, timeout: Duration) {
    let start = Instant::now();
    while current_thread_count() > target_threads && start.elapsed() < timeout {
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_idle(child_pids: &[u32], target_threads: usize, start: Instant) {
    while start.elapsed() < CHILD_EXIT_TIMEOUT {
        let no_children = child_pids.iter().all(|pid| !pid_alive(*pid));
        let threads_ok = current_thread_count() <= target_threads;
        if no_children && threads_ok {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn remove_dir_all_retry(path: &PathBuf, retries: usize) {
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

fn pid_alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn terminate_pid(pid: u32, signal: i32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let rc = unsafe { kill(pid as i32, signal) };
        if rc == 0 || !pid_alive(pid) {
            return Ok(());
        }
        return Err(std::io::Error::last_os_error());
    }

    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
        Ok(())
    }
}

fn reap_pid(pid: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        let mut status = 0;
        let rc = unsafe { waitpid(pid as i32, &mut status, 0) };
        if rc >= 0 || !pid_alive(pid) {
            return Ok(());
        }
        return Err(std::io::Error::last_os_error());
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(())
    }
}

fn close_fd(fd: i32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn close(fd: i32) -> i32;
        }
        let rc = unsafe { close(fd) };
        if rc == 0 {
            return Ok(());
        }
        return Err(std::io::Error::last_os_error());
    }

    #[cfg(not(unix))]
    {
        let _ = fd;
        Ok(())
    }
}

#[cfg(test)]
fn duplicate_fd(fd: i32) -> std::io::Result<i32> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn dup(fd: i32) -> i32;
        }
        let duplicated = unsafe { dup(fd) };
        if duplicated >= 0 {
            return Ok(duplicated);
        }
        return Err(std::io::Error::last_os_error());
    }

    #[cfg(not(unix))]
    {
        let _ = fd;
        Err(std::io::Error::other("fd duplication unsupported"))
    }
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    #[test]
    fn resource_safe_scope_passes_without_leaks() {
        let snapshot = ResourceSnapshot::default();
        validate_untracked_resources(&snapshot, &snapshot, &Thresholds::default())
            .expect("zero delta should pass");
    }

    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        payload
            .downcast_ref::<&str>()
            .copied()
            .map(str::to_string)
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default()
    }

    #[test]
    #[should_panic(expected = "Tracked resource leak detected")]
    fn t_leak_1_process_leak_fail() {
        run_resource_safe_test("process_leak", || {
            let child = Command::new("/bin/sh")
                .args(["-c", "sleep 30"])
                .spawn()
                .expect("spawn child");
            register_current_child(child.id());
            std::thread::sleep(Duration::from_millis(75));
            std::mem::forget(child);
        });
    }

    #[test]
    #[should_panic(expected = "Tracked resource leak detected")]
    fn t_leak_2_zombie_fail() {
        run_resource_safe_test("zombie_leak", || {
            let child = Command::new("/bin/sh")
                .args(["-c", "exit 0"])
                .spawn()
                .expect("spawn child");
            register_current_child(child.id());
            std::thread::sleep(Duration::from_millis(100));
            std::mem::forget(child);
        });
    }

    #[test]
    #[should_panic(expected = "Tracked resource leak detected")]
    fn t_leak_3_fd_leak_fail() {
        run_resource_safe_test("fd_leak", || {
            let path = std::env::temp_dir().join(format!("dbm_fd_leak_{}", std::process::id()));
            fs::write(&path, "fd").expect("write temp file");
            for _ in 0..8 {
                let file = fs::File::open(&path).expect("open file");
                register_current_fd(file.as_raw_fd());
                std::mem::forget(file);
            }
            std::thread::sleep(Duration::from_millis(25));
            let _ = fs::remove_file(path);
        });
    }

    #[test]
    fn t_rs_guard_1_untracked_process_generation_fails() {
        let leaked_pid = Arc::new(Mutex::new(None));
        let leaked_pid_for_test = Arc::clone(&leaked_pid);
        let result = panic::catch_unwind(move || {
            run_resource_safe_test("untracked_process", || {
                let child = Command::new("/bin/sh")
                    .args(["-c", "sleep 30"])
                    .spawn()
                    .expect("spawn child");
                *leaked_pid_for_test.lock().expect("pid mutex") = Some(child.id());
                std::thread::sleep(Duration::from_millis(75));
                std::mem::forget(child);
            });
        });
        let message = panic_message(result.expect_err("guard should fail"));
        assert!(message.contains("Untracked resource leak detected"));
        if let Some(pid) = *leaked_pid.lock().expect("pid mutex") {
            let _ = terminate_pid(pid, 15);
            let _ = terminate_pid(pid, 9);
            let _ = reap_pid(pid);
        }
    }

    #[test]
    fn t_rs_guard_2_untracked_fd_generation_fails() {
        let leaked_fds = Arc::new(Mutex::new(Vec::new()));
        let leaked_fds_for_test = Arc::clone(&leaked_fds);
        let result = panic::catch_unwind(move || {
            run_resource_safe_test("untracked_fd", || {
                let path =
                    std::env::temp_dir().join(format!("dbm_untracked_fd_{}", std::process::id()));
                fs::write(&path, "fd").expect("write temp file");
                let file = fs::File::open(&path).expect("open file");
                let source_fd = file.as_raw_fd();
                for _ in 0..8 {
                    let fd = duplicate_fd(source_fd).expect("dup fd");
                    leaked_fds_for_test.lock().expect("fd mutex").push(fd);
                }
                std::thread::sleep(Duration::from_millis(25));
                let _ = fs::remove_file(path);
            });
        });
        let message = panic_message(result.expect_err("guard should fail"));
        assert!(message.contains("Untracked resource leak detected"));
        for fd in leaked_fds.lock().expect("fd mutex").drain(..) {
            let _ = close_fd(fd);
        }
    }

    #[test]
    fn t_rs_guard_3_untracked_task_generation_fails() {
        let leaked_task = Arc::new(Mutex::new(None));
        let leaked_task_for_test = Arc::clone(&leaked_task);
        let result = panic::catch_unwind(move || {
            run_resource_safe_test("untracked_task", || {
                let (tx, rx) = mpsc::channel::<()>();
                let path = std::env::temp_dir()
                    .join(format!("dbm_untracked_task_fd_{}", std::process::id()));
                fs::write(&path, "task").expect("write temp file");
                let handle = thread::spawn(move || {
                    let mut files = Vec::new();
                    for _ in 0..4 {
                        let file = fs::File::open(&path).expect("open file");
                        let _ = file.as_raw_fd();
                        files.push(file);
                    }
                    let _ = rx.recv_timeout(Duration::from_secs(5));
                });
                *leaked_task_for_test.lock().expect("task mutex") = Some((tx, handle));
                std::thread::sleep(Duration::from_millis(75));
            });
        });
        let message = panic_message(result.expect_err("guard should fail"));
        assert!(message.contains("Untracked resource leak detected"));
        if let Some((tx, handle)) = leaked_task.lock().expect("task mutex").take() {
            let _ = tx.send(());
            let _ = handle.join();
        }
    }

    #[test]
    fn t_rs_guard_noise_normal_execution_passes() {
        run_resource_safe_test("noise_tolerance", || {
            let before = capture_snapshot();
            std::thread::sleep(Duration::from_millis(10));
            let after = capture_snapshot();
            validate_untracked_resources(&before, &after, &Thresholds::default())
                .expect("delta should remain within thresholds");
        });
    }

    #[test]
    fn t_fail_1_failure_after_cleanup_has_no_residual_process() {
        let leaked_pid = Arc::new(Mutex::new(None));
        let leaked_pid_for_test = Arc::clone(&leaked_pid);
        let result = panic::catch_unwind(|| {
            run_resource_safe_test("panic_cleanup", || {
                let child = Command::new("/bin/sh")
                    .args(["-c", "sleep 30"])
                    .spawn()
                    .expect("spawn child");
                register_current_child(child.id());
                *leaked_pid_for_test.lock().expect("pid mutex") = Some(child.id());
                std::mem::forget(child);
                panic!("boom");
            });
        });
        let message = panic_message(result.expect_err("panic should propagate"));
        assert!(
            message == "boom" || message.contains("Untracked resource leak detected"),
            "unexpected panic message: {message}"
        );
        if let Some(pid) = *leaked_pid.lock().expect("pid mutex") {
            assert!(
                !pid_alive(pid),
                "registered child {pid} should be cleaned up"
            );
        }
    }
}
