use crate::git::commands::{GitCommand, parse_git_command};

pub fn route_git_command(input: &str) -> Option<Result<GitCommand, String>> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("git") || trimmed.to_ascii_lowercase().starts_with("git ") {
        Some(parse_git_command(trimmed))
    } else {
        None
    }
}
