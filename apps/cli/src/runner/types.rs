use serde::ser::{Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionTarget {
    RustCargo,
    NodeScript(String),
    PythonModule(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowedCommand {
    Cargo,
    Npm,
    Node,
    Python,
    DotNet,
}

impl AllowedCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Npm => "npm",
            Self::Node => "node",
            Self::Python => "python",
            Self::DotNet => "dotnet",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SandboxMode {
    FullCopy,
    Incremental,
    Reuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Buffered,
    Streaming,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SandboxKey {
    pub path_hash: u64,
    pub file_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConfig {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub timeout_ms: u64,
    pub env: Vec<(String, String)>,
    pub clean_env: bool,
    pub output_mode: OutputMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutConfig {
    pub timeout_ms: u64,
    pub kill_signal: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub allow_network: bool,
    pub allow_fs_write: bool,
    pub allowed_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OutputMeta {
    pub streamed: bool,
    pub truncated: bool,
    pub original_size: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryUsage {
    Known(u64),
    Unknown,
}

impl Serialize for MemoryUsage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Known(value) => serializer.serialize_u64(*value),
            Self::Unknown => serializer.serialize_str("unknown"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CpuReleaseTelemetry {
    pub baseline_threads: usize,
    pub final_threads: usize,
    pub child_processes_after: usize,
    pub cpu_idle_recovery_ms: u64,
    pub zombie_detected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Telemetry {
    pub duration_ms: u128,
    pub exit_code: i32,
    pub stdout_size: usize,
    pub stderr_size: usize,
    pub memory_usage_kb: MemoryUsage,
    pub cpu_release: CpuReleaseTelemetry,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ExitStatus {
    Code(i32),
    Signaled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub status: String,
    pub exit_code: i32,
    pub exit_status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub telemetry: Telemetry,
    pub output_meta: OutputMeta,
    pub stderr_meta: OutputMeta,
    pub sandbox_mode: SandboxMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerError {
    ValidationError(String),
    TimeoutError(String),
    ExecutionError(String),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationError(message) => write!(f, "ValidationError: {message}"),
            Self::TimeoutError(message) => write!(f, "TimeoutError: {message}"),
            Self::ExecutionError(message) => write!(f, "ExecutionError: {message}"),
        }
    }
}

impl std::error::Error for RunnerError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerResult {
    Success(ExecutionResult),
    Panic,
}

#[derive(Debug)]
pub struct SandboxGuard {
    path: std::path::PathBuf,
}

impl SandboxGuard {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for SandboxGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
pub struct SandboxInstance {
    pub guard: SandboxGuard,
    pub mode: SandboxMode,
    pub key: SandboxKey,
}

#[derive(Clone, Debug)]
pub(crate) struct SandboxCacheEntry {
    pub key: SandboxKey,
    pub cache_dir: std::path::PathBuf,
}
