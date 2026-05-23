use crate::error::HardeningError;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;

/// Maximum bytes captured from stdout.  Outputs beyond this limit are truncated.
pub const DEFAULT_STDOUT_LIMIT_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Shell binaries that are absolutely forbidden.
/// Executing a shell bypasses argument pre-splitting and enables arbitrary injection.
const FORBIDDEN_BINARIES: &[&str] = &[
    "sh", "bash", "zsh", "fish", "ksh", "tcsh", "csh", "dash", "ash",
];

/// Arguments that enable inline shell execution — forbidden regardless of binary.
const FORBIDDEN_ARG_PATTERNS: &[&str] = &["-c", "--command", "-e"];

/// Output captured from a `SandboxedCommand` run.
#[derive(Debug, Clone)]
pub struct SandboxedOutput {
    /// Captured standard output (may be truncated to `stdout_limit` bytes).
    pub stdout: String,
    /// Whether the process exited with a success status code.
    pub success: bool,
    /// OS-level exit code, if available.
    pub exit_code: Option<i32>,
}

/// A command builder that enforces Phase C.5 sandbox constraints:
///
/// - Fixed working directory (caller-supplied)
/// - Full environment clear (`env_clear()`)
/// - Standard input disabled (`Stdio::null()`)
/// - Arguments must be pre-split (no shell string interpolation)
/// - Shell binaries and `-c` / `--command` flags are rejected at build time
/// - Only stdout is captured; stderr is discarded
/// - Stdout is truncated at `stdout_limit` bytes
///
/// Spec §5 ExternalCommand Sandbox
#[derive(Debug, Clone)]
pub struct SandboxedCommand {
    binary: PathBuf,
    args: Vec<OsString>,
    working_dir: PathBuf,
    stdout_limit: usize,
}

impl SandboxedCommand {
    /// Create a new sandboxed command.
    ///
    /// `binary` must be a single executable path or name — no shell strings.
    /// `working_dir` is the fixed working directory for the process.
    ///
    /// Returns `Err(SandboxViolation)` when `binary` is a forbidden shell.
    pub fn new(
        binary: impl Into<PathBuf>,
        working_dir: impl Into<PathBuf>,
    ) -> Result<Self, HardeningError> {
        let binary: PathBuf = binary.into();
        let working_dir: PathBuf = working_dir.into();

        let name = binary.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if FORBIDDEN_BINARIES.contains(&name) {
            return Err(HardeningError::SandboxViolation(format!(
                "Forbidden binary '{name}': shell execution is prohibited (spec §5.5)"
            )));
        }

        Ok(Self {
            binary,
            args: Vec::new(),
            working_dir,
            stdout_limit: DEFAULT_STDOUT_LIMIT_BYTES,
        })
    }

    /// Append a single pre-split argument.
    ///
    /// Returns `Err(SandboxViolation)` if the argument matches a forbidden
    /// inline-execution flag such as `-c` or `--command`.
    pub fn arg(mut self, arg: impl Into<OsString>) -> Result<Self, HardeningError> {
        let arg: OsString = arg.into();
        if let Some(s) = arg.to_str()
            && FORBIDDEN_ARG_PATTERNS.contains(&s)
        {
            return Err(HardeningError::SandboxViolation(format!(
                "Forbidden argument '{s}': dynamic shell execution is prohibited (spec §5.5)"
            )));
        }
        self.args.push(arg);
        Ok(self)
    }

    /// Append multiple pre-split arguments.
    pub fn args<I, S>(mut self, args: I) -> Result<Self, HardeningError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        for arg in args {
            self = self.arg(arg)?;
        }
        Ok(self)
    }

    /// Override the stdout size limit (default: 10 MiB).
    pub fn stdout_limit(mut self, bytes: usize) -> Self {
        self.stdout_limit = bytes;
        self
    }

    /// Execute the command under full sandbox constraints.
    ///
    /// Returns `Err(SandboxViolation)` if the OS refuses to spawn the process.
    pub fn run(&self) -> Result<SandboxedOutput, HardeningError> {
        // Build the command with all sandbox constraints applied.
        let output = std::process::Command::new(&self.binary)
            .args(&self.args)
            .current_dir(&self.working_dir)
            // Spec §5.1: env 完全クリア
            .env_clear()
            // Spec §5.1: 標準入力禁止
            .stdin(Stdio::null())
            // Spec §5.4: stdoutのみ取得
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| {
                HardeningError::SandboxViolation(format!(
                    "Failed to spawn '{}': {e}",
                    self.binary.display()
                ))
            })?;

        // Spec §5.4: サイズ制限あり
        let raw = if output.stdout.len() > self.stdout_limit {
            &output.stdout[..self.stdout_limit]
        } else {
            &output.stdout
        };

        Ok(SandboxedOutput {
            stdout: String::from_utf8_lossy(raw).into_owned(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    /// Return the binary path and collected args as a `Vec<String>` for logging.
    pub fn to_command_vec(&self) -> Vec<String> {
        std::iter::once(self.binary.to_string_lossy().into_owned())
            .chain(self.args.iter().map(|a| a.to_string_lossy().into_owned()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn rejects_shell_binary_sh() {
        let err = SandboxedCommand::new("sh", tmp_dir()).unwrap_err();
        assert!(matches!(err, HardeningError::SandboxViolation(_)));
    }

    #[test]
    fn rejects_shell_binary_bash() {
        let err = SandboxedCommand::new("bash", tmp_dir()).unwrap_err();
        assert!(matches!(err, HardeningError::SandboxViolation(_)));
    }

    #[test]
    fn rejects_dash_c_argument() {
        let cmd = SandboxedCommand::new("echo", tmp_dir()).unwrap();
        let err = cmd.arg("-c").unwrap_err();
        assert!(matches!(err, HardeningError::SandboxViolation(_)));
    }

    #[test]
    fn rejects_double_dash_command_argument() {
        let cmd = SandboxedCommand::new("echo", tmp_dir()).unwrap();
        let err = cmd.arg("--command").unwrap_err();
        assert!(matches!(err, HardeningError::SandboxViolation(_)));
    }

    #[test]
    fn accepts_safe_binary_and_args() {
        let cmd = SandboxedCommand::new("echo", tmp_dir())
            .unwrap()
            .args(["hello", "world"])
            .unwrap();
        assert_eq!(cmd.to_command_vec(), vec!["echo", "hello", "world"]);
    }

    #[test]
    fn run_echo_succeeds() {
        let output = SandboxedCommand::new("echo", tmp_dir())
            .unwrap()
            .arg("sandbox-ok")
            .unwrap()
            .run()
            .unwrap();
        assert!(output.success);
        // echo output goes to stdout — env_clear keeps PATH absent, but
        // the absolute "echo" binary should work on most Unix systems.
        // On systems where the shell built-in is the only echo, this may
        // not produce output — that is acceptable; we only test success.
    }
}
