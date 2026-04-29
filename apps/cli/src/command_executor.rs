//! Command Execution Layer (DBM-EXECUTOR-FULL-ISOLATION-SPEC v1.0)
//!
//! Responsible for executing external CLI commands.
//! Isolated from IR execution to ensure determinism of the core engine.

use std::path::PathBuf;
use std::process::Command;

/// Executes a design_cli subcommand as a subprocess.
pub fn run_command(subcommand: &str, args: &[String]) -> Result<String, String> {
    let exe = current_exe()?;

    let output = Command::new(&exe)
        .arg(subcommand)
        .args(args)
        .output()
        .map_err(|e| format!("Execution failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

    // info:/warn: lines from stderr are kept for visibility
    let info_lines: Vec<&str> = stderr_str
        .lines()
        .filter(|l| l.starts_with("info:") || l.starts_with("warn:"))
        .collect();

    if output.status.success() {
        let mut result = String::new();
        if !info_lines.is_empty() {
            result.push_str(&info_lines.join("\n"));
            result.push('\n');
        }
        result.push_str(&stdout);
        Ok(result)
    } else {
        let err_raw = String::from_utf8_lossy(&output.stderr).to_string();
        let err = if !err_raw.trim().is_empty() {
            err_raw
        } else {
            stdout
        };
        Err(err.trim().to_string())
    }
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Executable not found: {e}"))
}
