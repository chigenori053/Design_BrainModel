use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRejection {
    NoAppliedPlan,
    NoValidationPlan,
    UnsupportedValidationCommand(String),
    UnsafeValidationCommand(String),
    ValidationFailed(String),
}

impl ValidationRejection {
    pub fn as_str(&self) -> String {
        match self {
            Self::NoAppliedPlan => "no applied plan".to_string(),
            Self::NoValidationPlan => "no validation plan".to_string(),
            Self::UnsupportedValidationCommand(cmd) => {
                format!("unsupported validation command: {cmd}")
            }
            Self::UnsafeValidationCommand(cmd) => format!("unsafe validation command: {cmd}"),
            Self::ValidationFailed(cmd) => format!("validation failed: {cmd}"),
        }
    }
}

pub fn parse_validation_command(line: &str) -> String {
    let prefixes = [
        "Validation:",
        "validation:",
        "Test:",
        "test:",
        "Validate:",
        "validate:",
    ];

    let mut command = line.trim();
    for prefix in prefixes {
        if command.starts_with(prefix) {
            command = command[prefix.len()..].trim();
            break;
        }
    }
    command.to_string()
}

pub fn is_safe_command(command: &str) -> bool {
    let unsafe_chars = [';', '&', '|', '>', '<', '`', '$'];
    if command.chars().any(|c| unsafe_chars.contains(&c)) {
        return false;
    }
    true
}

pub fn validate_command_allowlist(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    match parts[0] {
        "cargo" => {
            if parts.len() < 2 {
                return false;
            }
            match parts[1] {
                "test" | "clippy" | "check" => true,
                "fmt" => parts.contains(&"--check"),
                _ => false,
            }
        }
        _ => false,
    }
}

pub fn run_validation_command(command_str: &str, workspace_root: &Path) -> Result<(), String> {
    let parts: Vec<&str> = command_str.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty command".to_string());
    }

    let program = parts[0];
    let args = &parts[1..];

    let mut child = Command::new(program)
        .args(args)
        .current_dir(workspace_root)
        .spawn()
        .map_err(|err| format!("failed to spawn {program}: {err}"))?;

    let status = child
        .wait()
        .map_err(|err| format!("failed to wait for {program}: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "command exited with non-zero status: {command_str}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_validation_command() {
        assert_eq!(parse_validation_command("cargo test"), "cargo test");
        assert_eq!(
            parse_validation_command("Validation: cargo test"),
            "cargo test"
        );
        assert_eq!(
            parse_validation_command("Test: cargo clippy"),
            "cargo clippy"
        );
        assert_eq!(
            parse_validation_command("Validate: cargo check"),
            "cargo check"
        );
        assert_eq!(
            parse_validation_command("  validation:   cargo fmt  "),
            "cargo fmt"
        );
    }

    #[test]
    fn test_is_safe_command() {
        assert!(is_safe_command("cargo test"));
        assert!(!is_safe_command("cargo test && rm -rf /"));
        assert!(!is_safe_command("cargo test ; ls"));
        assert!(!is_safe_command("cargo test | grep error"));
        assert!(!is_safe_command("cargo test > output.txt"));
        assert!(!is_safe_command("echo `id`"));
        assert!(!is_safe_command("echo $(id)"));
    }

    #[test]
    fn test_validate_command_allowlist() {
        assert!(validate_command_allowlist("cargo test"));
        assert!(validate_command_allowlist("cargo test -p design_cli"));
        assert!(validate_command_allowlist("cargo clippy --all-targets"));
        assert!(validate_command_allowlist("cargo check"));
        assert!(validate_command_allowlist("cargo fmt --check"));
        assert!(validate_command_allowlist(
            "cargo fmt -p design_cli --check"
        ));

        assert!(!validate_command_allowlist("cargo fmt"));
        assert!(!validate_command_allowlist("cargo run"));
        assert!(!validate_command_allowlist("rm -rf /"));
        assert!(!validate_command_allowlist("git status"));
        assert!(!validate_command_allowlist(""));
    }
}
