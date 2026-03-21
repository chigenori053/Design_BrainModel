use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use super::types::RunnerError;

pub struct CommandResolver {
    cache: HashMap<String, String>,
    overrides: HashMap<String, String>,
}

impl CommandResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            overrides: HashMap::new(),
        }
    }

    pub fn with_overrides(overrides: HashMap<String, String>) -> Self {
        Self {
            cache: HashMap::new(),
            overrides,
        }
    }

    pub fn set_override(&mut self, command: impl Into<String>, path: impl Into<String>) {
        self.overrides.insert(command.into(), path.into());
    }

    pub fn resolve(&mut self, cmd: &str) -> Result<String, RunnerError> {
        if let Some(path) = self.overrides.get(cmd) {
            return validate_resolved_absolute_path(path);
        }
        if let Some(path) = self.cache.get(cmd) {
            return validate_resolved_absolute_path(path);
        }

        let output = std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map_err(|_| RunnerError::ValidationError("which failed".into()))?;

        if !output.status.success() {
            return Err(RunnerError::ValidationError(format!(
                "command not found: {cmd}"
            )));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = validate_resolved_absolute_path(&path)?;
        self.cache.insert(cmd.to_string(), path.clone());
        Ok(path)
    }
}

pub fn resolve_command(command: &str) -> Result<String, RunnerError> {
    let mut resolver = global_resolver()
        .lock()
        .map_err(|_| RunnerError::ExecutionError("command resolver lock poisoned".to_string()))?;
    resolver.resolve(command)
}

pub fn set_command_override(command: &str, path: &str) -> Result<(), RunnerError> {
    let mut resolver = global_resolver()
        .lock()
        .map_err(|_| RunnerError::ExecutionError("command resolver lock poisoned".to_string()))?;
    resolver.set_override(command, path);
    Ok(())
}

fn global_resolver() -> &'static Mutex<CommandResolver> {
    static RESOLVER: OnceLock<Mutex<CommandResolver>> = OnceLock::new();
    RESOLVER.get_or_init(|| Mutex::new(CommandResolver::new()))
}

fn validate_resolved_absolute_path(path: &str) -> Result<String, RunnerError> {
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(RunnerError::ValidationError(format!(
            "resolved command path must be absolute: {path}"
        )));
    }
    if !candidate.exists() {
        return Err(RunnerError::ValidationError(format!(
            "resolved command path does not exist: {path}"
        )));
    }
    Ok(path.to_string())
}
