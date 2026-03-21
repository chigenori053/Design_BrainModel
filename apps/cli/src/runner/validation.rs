use std::path::{Component, Path, PathBuf};

use super::SandboxPolicy;
use super::types::RunnerError;

pub(crate) fn validate_resolved_command(command: &str) -> Result<(), RunnerError> {
    let file_name = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            RunnerError::ValidationError(format!("invalid resolved command path: {command}"))
        })?;

    match file_name {
        "cargo" | "node" | "python" => Ok(()),
        _ => Err(RunnerError::ValidationError(format!(
            "command is not allowed by runner policy: {file_name}"
        ))),
    }
}

pub(crate) fn validate_args(args: &[String]) -> Result<(), RunnerError> {
    for arg in args {
        if arg.contains(';')
            || arg.contains("&&")
            || arg.contains('|')
            || arg.contains("$(")
            || arg.contains('`')
        {
            return Err(RunnerError::ValidationError(format!(
                "argument contains forbidden shell syntax: {arg}"
            )));
        }
        if arg == ".." || arg.starts_with("../") || arg.contains("/../") {
            return Err(RunnerError::ValidationError(format!(
                "argument contains forbidden parent path traversal: {arg}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_working_dir(input: &str, root: &Path) -> Result<PathBuf, RunnerError> {
    let input_path = Path::new(input);
    if contains_parent_component(input_path) {
        return Err(RunnerError::ValidationError(format!(
            "working directory contains forbidden parent traversal: {input}"
        )));
    }
    let canonical = canonical_dir(input_path).map_err(RunnerError::ValidationError)?;
    if !canonical.starts_with(root) {
        return Err(RunnerError::ValidationError(format!(
            "working directory escapes sandbox root: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

pub(crate) fn validate_allowed_paths(
    policy: &SandboxPolicy,
    root: &Path,
    working_dir: &Path,
) -> Result<(), RunnerError> {
    if policy.allowed_paths.is_empty() {
        return Ok(());
    }

    let mut allowed = Vec::new();
    for path in &policy.allowed_paths {
        let canonical = canonical_dir(Path::new(path)).map_err(RunnerError::ValidationError)?;
        if !canonical.starts_with(root) {
            return Err(RunnerError::ValidationError(format!(
                "allowed path escapes project root: {}",
                canonical.display()
            )));
        }
        allowed.push(canonical);
    }

    if allowed
        .iter()
        .any(|allowed| working_dir.starts_with(allowed))
    {
        Ok(())
    } else {
        Err(RunnerError::ValidationError(format!(
            "working directory is not in allowed paths: {}",
            working_dir.display()
        )))
    }
}

pub(crate) fn canonical_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|err| format!("failed to resolve {}: {err}", path.display()))?;
    if !canonical.is_absolute() {
        return Err(format!(
            "path must resolve to an absolute directory: {}",
            canonical.display()
        ));
    }
    if !canonical.is_dir() {
        return Err(format!("path is not a directory: {}", canonical.display()));
    }
    Ok(canonical)
}

fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}
