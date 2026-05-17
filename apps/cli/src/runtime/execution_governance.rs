use crate::runtime::shell::ResolvedExecutionTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CommandType {
    SafeRead,
    SafeWrite,
    Dangerous,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    DryRun,
    GovernedExecute,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPolicy {
    pub command_type: CommandType,
    pub requires_confirmation: bool,
    pub allow_remote_execution: bool,
    pub allow_filesystem_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub semantic_intent: String,
    pub resolved_target: ResolvedExecutionTarget,
    pub command: String,
    pub command_type: CommandType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionValidation {
    pub allowed: bool,
    pub policy: CommandPolicy,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionAuditRecord {
    pub timestamp: u64,
    pub command: String,
    pub command_type: CommandType,
    pub semantic_hash: String,
    pub projection_hash: String,
    pub approved: bool,
    pub execution_result: ExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionState {
    DryRun,
    Allowed,
    Rejected,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionProjection {
    pub execution_state: ExecutionState,
    pub command_summary: String,
    pub audit_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaltTrigger {
    PolicyViolation,
    SemanticMismatch,
    ForbiddenCommand,
    StaleProjection,
    TargetMutation,
}

pub fn classify_command(command: &str) -> CommandType {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["git", "status"]
        | ["git", "status", "--porcelain"]
        | ["git", "diff"]
        | ["git", "log"]
        | ["git", "log", "--oneline"] => CommandType::SafeRead,
        ["gh", "pr", "view", ..] | ["gh", "status"] | ["gh", "auth", "status"] => {
            CommandType::SafeRead
        }
        ["ls", ..] | ["cat", _] | ["grep", ..] => CommandType::SafeRead,
        ["git", "add", path] if is_explicit_file(path) => CommandType::SafeWrite,
        ["git", "add", "--", path] if is_explicit_file(path) => CommandType::SafeWrite,
        ["git", "commit"] => CommandType::SafeWrite,
        ["git", "commit", "-m", ..] => CommandType::SafeWrite,
        ["cargo", "fmt"] | ["cargo", "test"] => CommandType::SafeWrite,
        ["git", "push", "--force", ..]
        | ["git", "push", "-f", ..]
        | ["gh", "repo", "delete", ..]
        | ["gh", "secret", "set", ..]
        | ["sudo", "rm", ..]
        | ["shutdown", ..]
        | ["reboot", ..]
        | ["rm", "-rf", "/"] => CommandType::Forbidden,
        ["git", "push", ..] | ["gh", "pr", "create", ..] => CommandType::Dangerous,
        ["git", "add", "."]
        | ["git", "commit", "--amend", ..]
        | ["git", "rebase", ..]
        | ["git", "reset", ..]
        | ["git", "clean", ..] => CommandType::Forbidden,
        _ => CommandType::Forbidden,
    }
}

pub fn command_policy(command_type: CommandType) -> CommandPolicy {
    match command_type {
        CommandType::SafeRead => CommandPolicy {
            command_type,
            requires_confirmation: false,
            allow_remote_execution: true,
            allow_filesystem_mutation: false,
        },
        CommandType::SafeWrite => CommandPolicy {
            command_type,
            requires_confirmation: false,
            allow_remote_execution: false,
            allow_filesystem_mutation: true,
        },
        CommandType::Dangerous => CommandPolicy {
            command_type,
            requires_confirmation: true,
            allow_remote_execution: true,
            allow_filesystem_mutation: true,
        },
        CommandType::Forbidden => CommandPolicy {
            command_type,
            requires_confirmation: false,
            allow_remote_execution: false,
            allow_filesystem_mutation: false,
        },
    }
}

pub fn validate_execution_request(
    request: &ExecutionRequest,
    mode: ExecutionMode,
    expected_target: &ResolvedExecutionTarget,
) -> ExecutionValidation {
    let mut errors = Vec::new();
    let classified = classify_command(&request.command);
    let policy = command_policy(classified);

    if mode == ExecutionMode::Halted {
        errors.push("execution is halted".to_string());
    }
    if classified != request.command_type {
        errors.push("command type mismatch".to_string());
    }
    if classified == CommandType::Forbidden {
        errors.push("forbidden command rejected".to_string());
    }
    if request.resolved_target != *expected_target {
        errors.push("resolved execution target mutated".to_string());
    }
    if !semantic_intent_matches_command(&request.semantic_intent, &request.command) {
        errors.push("semantic intent does not match command".to_string());
    }

    errors.sort();
    errors.dedup();
    ExecutionValidation {
        allowed: errors.is_empty() && mode != ExecutionMode::DryRun,
        policy,
        errors,
    }
}

pub fn audit_execution(
    request: &ExecutionRequest,
    projection_hash: &str,
    approved: bool,
    execution_result: ExecutionResult,
) -> ExecutionAuditRecord {
    ExecutionAuditRecord {
        timestamp: deterministic_audit_timestamp(
            &request.command,
            &request.resolved_target.semantic_hash,
            projection_hash,
        ),
        command: request.command.clone(),
        command_type: request.command_type,
        semantic_hash: request.resolved_target.semantic_hash.clone(),
        projection_hash: projection_hash.to_string(),
        approved,
        execution_result,
    }
}

pub fn execution_projection(record: &ExecutionAuditRecord) -> ExecutionProjection {
    ExecutionProjection {
        execution_state: if record.approved {
            ExecutionState::Allowed
        } else if record.command_type == CommandType::Forbidden {
            ExecutionState::Halted
        } else {
            ExecutionState::Rejected
        },
        command_summary: semantic_command_summary(&record.command),
        audit_reference: format!(
            "{:016x}",
            stable_hash_strs([record.command.as_str(), record.semantic_hash.as_str()])
        ),
    }
}

pub fn semantic_intent_matches_command(intent: &str, command: &str) -> bool {
    let intent = intent.to_ascii_lowercase();
    let command = command.to_ascii_lowercase();
    if intent.contains("test") || intent.contains("テスト") {
        return command == "cargo test";
    }
    if intent.contains("format") || intent.contains("fmt") {
        return command == "cargo fmt";
    }
    if intent.contains("status") {
        return command == "git status" || command == "gh status" || command == "gh auth status";
    }
    if intent.contains("diff") {
        return command == "git diff";
    }
    if intent.contains("log") {
        return command == "git log";
    }
    if intent.contains("commit") {
        return command == "git commit" || command.starts_with("git commit -m ");
    }
    if intent.contains("stage") || intent.contains("add") {
        return command.starts_with("git add ");
    }
    if intent.contains("pull request") || intent.contains("pr") {
        return command.starts_with("gh pr view") || command.starts_with("gh pr create");
    }
    true
}

fn is_explicit_file(path: &str) -> bool {
    !path.is_empty()
        && path != "."
        && !path.starts_with('-')
        && !path.contains('*')
        && !path.contains('?')
        && !std::path::Path::new(path).is_absolute()
        && !std::path::Path::new(path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn semantic_command_summary(command: &str) -> String {
    match classify_command(command) {
        CommandType::SafeRead => "safe read command".to_string(),
        CommandType::SafeWrite => "governed write command".to_string(),
        CommandType::Dangerous => "confirmation required command".to_string(),
        CommandType::Forbidden => "forbidden command rejected".to_string(),
    }
}

fn deterministic_audit_timestamp(command: &str, semantic_hash: &str, projection_hash: &str) -> u64 {
    stable_hash_strs([command, semantic_hash, projection_hash])
}

fn stable_hash_strs<'a>(values: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in values {
        hash ^= value.len() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::shell::ResolvedExecutionTarget;

    fn target() -> ResolvedExecutionTarget {
        ResolvedExecutionTarget::from_canonical_path("apps/cli/src/core.rs")
    }

    #[test]
    fn forbidden_command_is_rejected() {
        let target = target();
        let request = ExecutionRequest {
            semantic_intent: "run tests".to_string(),
            resolved_target: target.clone(),
            command: "git push --force origin main".to_string(),
            command_type: classify_command("git push --force origin main"),
        };

        let validation =
            validate_execution_request(&request, ExecutionMode::GovernedExecute, &target);

        assert!(!validation.allowed);
        assert_eq!(validation.policy.command_type, CommandType::Forbidden);
        assert!(
            validation
                .errors
                .contains(&"forbidden command rejected".to_string())
        );
    }

    #[test]
    fn semantic_mismatch_is_rejected() {
        let target = target();
        let request = ExecutionRequest {
            semantic_intent: "run tests".to_string(),
            resolved_target: target.clone(),
            command: "git push origin feature".to_string(),
            command_type: classify_command("git push origin feature"),
        };

        let validation =
            validate_execution_request(&request, ExecutionMode::GovernedExecute, &target);

        assert!(!validation.allowed);
        assert!(
            validation
                .errors
                .contains(&"semantic intent does not match command".to_string())
        );
    }

    #[test]
    fn immutable_target_mismatch_is_rejected() {
        let expected = target();
        let request = ExecutionRequest {
            semantic_intent: "run tests".to_string(),
            resolved_target: ResolvedExecutionTarget::from_canonical_path("apps/cli/src/repl.rs"),
            command: "cargo test".to_string(),
            command_type: classify_command("cargo test"),
        };

        let validation =
            validate_execution_request(&request, ExecutionMode::GovernedExecute, &expected);

        assert!(!validation.allowed);
        assert!(
            validation
                .errors
                .contains(&"resolved execution target mutated".to_string())
        );
    }

    #[test]
    fn same_input_produces_same_audit_record() {
        let target = target();
        let request = ExecutionRequest {
            semantic_intent: "run tests".to_string(),
            resolved_target: target,
            command: "cargo test".to_string(),
            command_type: classify_command("cargo test"),
        };
        let result = ExecutionResult {
            status: "ok".to_string(),
            summary: "tests passed".to_string(),
        };

        let first = audit_execution(&request, "projection-a", true, result.clone());
        let second = audit_execution(&request, "projection-a", true, result);

        assert_eq!(first, second);
        assert_eq!(execution_projection(&first), execution_projection(&second));
    }
}
