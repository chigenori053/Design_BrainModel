use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Serialize;

use crate::runner::{
    ExecutionConfig, OutputMode, SandboxPolicy, TimeoutConfig, create_sandbox, fixed_env,
    resolve_command, run as run_command,
};
use crate::runner::{OutputMeta, SandboxMode, Telemetry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    DotNet,
    Unknown,
}

impl ProjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::DotNet => "dotnet",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ExecAction {
    Detect,
    Install,
    Build,
    Test,
    Run,
}

impl ExecAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Detect => "detect",
            Self::Install => "install",
            Self::Build => "build",
            Self::Test => "test",
            Self::Run => "run",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSet {
    pub install: Option<Vec<String>>,
    pub build: Option<Vec<String>>,
    pub test: Option<Vec<String>>,
    pub run: Option<Vec<String>>,
}

impl CommandSet {
    pub fn command_for(&self, action: ExecAction) -> Option<&Vec<String>> {
        match action {
            ExecAction::Detect => None,
            ExecAction::Install => self.install.as_ref(),
            ExecAction::Build => self.build.as_ref(),
            ExecAction::Test => self.test.as_ref(),
            ExecAction::Run => self.run.as_ref(),
        }
    }
}

pub trait LanguageAdapter {
    fn detect(path: &Path) -> ProjectType;
    fn get_commands(project_type: ProjectType) -> CommandSet;
}

pub struct ExecutionFoundation;

impl LanguageAdapter for ExecutionFoundation {
    fn detect(path: &Path) -> ProjectType {
        detect_project_type(path)
    }

    fn get_commands(project_type: ProjectType) -> CommandSet {
        command_set_for(project_type)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecReport {
    pub root: String,
    pub project_type: ProjectType,
    pub action: ExecAction,
    pub status: String,
    pub success: bool,
    pub error_type: String,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub output_meta: OutputMeta,
    pub stderr_meta: OutputMeta,
    pub sandbox_mode: Option<SandboxMode>,
    pub telemetry: Option<Telemetry>,
    pub deterministic: bool,
}

impl ExecutionFoundation {
    pub fn detect(path: &Path) -> ProjectType {
        <Self as LanguageAdapter>::detect(path)
    }

    pub fn get_commands(project_type: ProjectType) -> CommandSet {
        <Self as LanguageAdapter>::get_commands(project_type)
    }

    pub fn execute(path: &Path, action: ExecAction, timeout_ms: u64) -> Result<ExecReport, String> {
        let canonical_root = canonical_project_root(path)?;
        let project_type = Self::detect(&canonical_root);
        if project_type == ProjectType::Unknown {
            return Err("Unsupported project type".to_string());
        }

        if action == ExecAction::Detect {
            return Ok(ExecReport {
                root: canonical_root.display().to_string(),
                project_type,
                action,
                status: "success".to_string(),
                success: true,
                error_type: "None".to_string(),
                exit_code: 0,
                duration_ms: 0,
                stdout: project_type.as_str().to_string(),
                stderr: String::new(),
                truncated: false,
                command: None,
                args: Vec::new(),
                output_meta: OutputMeta {
                    streamed: false,
                    truncated: false,
                    original_size: project_type.as_str().len(),
                },
                stderr_meta: OutputMeta {
                    streamed: false,
                    truncated: false,
                    original_size: 0,
                },
                sandbox_mode: None,
                telemetry: None,
                deterministic: true,
            });
        }

        let command_set = Self::get_commands(project_type);
        let command = command_set.command_for(action).cloned().ok_or_else(|| {
            format!(
                "{} command is not defined for {} project",
                action.as_str(),
                project_type.as_str()
            )
        })?;
        execute_command(&canonical_root, project_type, action, timeout_ms, command)
    }
}

pub fn format_exec_report(report: &ExecReport) -> String {
    let mut lines = vec![
        format!("Exec {}", report.action.as_str()),
        format!("Project: {}", report.project_type.as_str()),
        format!("Status: {}", report.status),
        format!("Exit code: {}", report.exit_code),
    ];
    if let Some(command) = &report.command {
        if report.args.is_empty() {
            lines.push(format!("Command: {command}"));
        } else {
            lines.push(format!("Command: {} {}", command, report.args.join(" ")));
        }
    }
    if !report.stdout.trim().is_empty() {
        lines.push("Stdout:".to_string());
        lines.push(report.stdout.trim_end().to_string());
    }
    if !report.stderr.trim().is_empty() {
        lines.push("Stderr:".to_string());
        lines.push(report.stderr.trim_end().to_string());
    }
    lines.join("\n")
}

fn execute_command(
    root: &Path,
    project_type: ProjectType,
    action: ExecAction,
    timeout_ms: u64,
    command: Vec<String>,
) -> Result<ExecReport, String> {
    let sandbox = create_sandbox(root).map_err(|err| err.to_string())?;
    let sandbox_path = sandbox.guard.path();
    let executable_name = command
        .first()
        .cloned()
        .ok_or_else(|| "command set is empty".to_string())?;
    let resolved_command = resolve_command(&executable_name).map_err(|err| err.to_string())?;
    let args = command[1..].to_vec();
    let config = ExecutionConfig {
        command: resolved_command.clone(),
        args: args.clone(),
        working_dir: sandbox_path.display().to_string(),
        timeout_ms,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Buffered,
    };
    let policy = SandboxPolicy {
        allow_network: matches!(action, ExecAction::Install),
        allow_fs_write: true,
        allowed_paths: vec![sandbox_path.display().to_string()],
    };
    let timeout = TimeoutConfig {
        timeout_ms,
        kill_signal: "kill".to_string(),
    };
    let result = run_command(&config, &timeout, &policy, sandbox_path, sandbox.mode)
        .map_err(|err| err.to_string())?;
    let status = result.status;
    let error_type = if status == "timeout" {
        "Timeout".to_string()
    } else if result.exit_code == 0 {
        "None".to_string()
    } else {
        "Unknown".to_string()
    };
    Ok(ExecReport {
        root: root.display().to_string(),
        project_type,
        action,
        status,
        success: result.exit_code == 0,
        error_type,
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        stdout: result.stdout,
        stderr: result.stderr,
        truncated: result.output_meta.truncated || result.stderr_meta.truncated,
        command: Some(resolved_command),
        args,
        output_meta: result.output_meta,
        stderr_meta: result.stderr_meta,
        sandbox_mode: Some(result.sandbox_mode),
        telemetry: Some(result.telemetry),
        deterministic: true,
    })
}

fn canonical_project_root(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|err| format!("failed to resolve project path {}: {err}", path.display()))
        .and_then(|canonical| {
            if canonical.is_dir() {
                Ok(canonical)
            } else {
                Err(format!("path is not a directory: {}", canonical.display()))
            }
        })
}

fn detect_project_type(path: &Path) -> ProjectType {
    let rust = path.join("Cargo.toml").exists();
    let node = path.join("package.json").exists();
    let python = path.join("requirements.txt").exists() || path.join("pyproject.toml").exists();
    let dotnet = has_csproj(path);

    if rust {
        ProjectType::Rust
    } else if node {
        ProjectType::Node
    } else if python {
        ProjectType::Python
    } else if dotnet {
        ProjectType::DotNet
    } else {
        ProjectType::Unknown
    }
}

fn has_csproj(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("csproj"))
            .unwrap_or(false)
    })
}

fn command_set_for(project_type: ProjectType) -> CommandSet {
    match project_type {
        ProjectType::Rust => CommandSet {
            install: None,
            build: Some(vec!["cargo".to_string(), "build".to_string()]),
            test: Some(vec!["cargo".to_string(), "test".to_string()]),
            run: Some(vec!["cargo".to_string(), "run".to_string()]),
        },
        ProjectType::Node => CommandSet {
            install: Some(vec!["npm".to_string(), "install".to_string()]),
            build: Some(vec![
                "npm".to_string(),
                "run".to_string(),
                "build".to_string(),
            ]),
            test: Some(vec!["npm".to_string(), "test".to_string()]),
            run: Some(vec!["npm".to_string(), "start".to_string()]),
        },
        ProjectType::Python => CommandSet {
            install: Some(vec![
                "python".to_string(),
                "-m".to_string(),
                "pip".to_string(),
                "install".to_string(),
                "-r".to_string(),
                "requirements.txt".to_string(),
            ]),
            build: None,
            test: Some(vec![
                "python".to_string(),
                "-m".to_string(),
                "pytest".to_string(),
            ]),
            run: Some(vec!["python".to_string(), "main.py".to_string()]),
        },
        ProjectType::DotNet => CommandSet {
            install: None,
            build: Some(vec!["dotnet".to_string(), "build".to_string()]),
            test: Some(vec!["dotnet".to_string(), "test".to_string()]),
            run: Some(vec!["dotnet".to_string(), "run".to_string()]),
        },
        ProjectType::Unknown => CommandSet {
            install: None,
            build: None,
            test: None,
            run: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("dbm_exec_foundation_{name}_{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detect_rust_with_highest_priority() {
        let path = temp_project("rust_priority");
        fs::write(
            path.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("requirements.txt"), "pytest\n").unwrap();
        assert_eq!(ExecutionFoundation::detect(&path), ProjectType::Rust);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn detect_node_project() {
        let path = temp_project("node");
        fs::write(path.join("package.json"), "{}").unwrap();
        assert_eq!(ExecutionFoundation::detect(&path), ProjectType::Node);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn detect_python_project() {
        let path = temp_project("python");
        fs::write(path.join("pyproject.toml"), "[project]\nname='demo'\n").unwrap();
        assert_eq!(ExecutionFoundation::detect(&path), ProjectType::Python);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn detect_dotnet_project() {
        let path = temp_project("dotnet");
        fs::write(path.join("demo.csproj"), "<Project />").unwrap();
        assert_eq!(ExecutionFoundation::detect(&path), ProjectType::DotNet);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn detect_unknown_project() {
        let path = temp_project("unknown");
        assert_eq!(ExecutionFoundation::detect(&path), ProjectType::Unknown);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn command_registry_matches_spec() {
        let rust = ExecutionFoundation::get_commands(ProjectType::Rust);
        assert_eq!(
            rust.build,
            Some(vec!["cargo".to_string(), "build".to_string()])
        );

        let node = ExecutionFoundation::get_commands(ProjectType::Node);
        assert_eq!(
            node.install,
            Some(vec!["npm".to_string(), "install".to_string()])
        );

        let python = ExecutionFoundation::get_commands(ProjectType::Python);
        assert_eq!(python.build, None);

        let dotnet = ExecutionFoundation::get_commands(ProjectType::DotNet);
        assert_eq!(
            dotnet.test,
            Some(vec!["dotnet".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn unsupported_project_returns_error() {
        let path = temp_project("unsupported");
        let error = ExecutionFoundation::execute(&path, ExecAction::Build, 1000).unwrap_err();
        assert_eq!(error, "Unsupported project type");
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn missing_python_build_command_returns_error() {
        let path = temp_project("python_build");
        fs::write(path.join("requirements.txt"), "pytest\n").unwrap();
        let error = ExecutionFoundation::execute(&path, ExecAction::Build, 1000).unwrap_err();
        assert!(error.contains("build command is not defined"));
        let _ = fs::remove_dir_all(path);
    }
}
