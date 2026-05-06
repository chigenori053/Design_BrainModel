const RUNTIME_COMMANDS: &[&str] = &[
    "undo",
    "preview",
    "apply",
    "rollback",
    "replay",
    "git status",
    "git diff",
];

pub fn complete_command(prefix: &str) -> Option<String> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut matches = RUNTIME_COMMANDS
        .iter()
        .filter(|command| command.starts_with(trimmed))
        .copied()
        .collect::<Vec<_>>();
    matches.sort();
    matches.first().map(|command| (*command).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_runtime_command() {
        assert_eq!(complete_command("git s"), Some("git status".to_string()));
    }
}
