use std::path::PathBuf;

pub const DBM_FIXED_COMMIT_MESSAGE: &str = "[DBM] apply verified change";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCommand {
    Status,
    Diff,
    Log,
    AddFile(PathBuf),
    Commit { message: String },
}

impl GitCommand {
    pub fn targets(&self) -> Vec<PathBuf> {
        match self {
            Self::AddFile(path) => vec![path.clone()],
            _ => Vec::new(),
        }
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Status => "git status --porcelain".to_string(),
            Self::Diff => "git diff".to_string(),
            Self::Log => "git log --oneline".to_string(),
            Self::AddFile(path) => format!("git add -- {}", path.display()),
            Self::Commit { .. } => "git commit -m [DBM_FIXED_MESSAGE]".to_string(),
        }
    }
}

pub fn parse_git_command(input: &str) -> Result<GitCommand, String> {
    let parts = input.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["git", "status"] => Ok(GitCommand::Status),
        ["git", "diff"] => Ok(GitCommand::Diff),
        ["git", "log"] => Ok(GitCommand::Log),
        ["git", "add", path] => {
            if *path == "." {
                return Err("git add . is rejected".to_string());
            }
            validate_scoped_add_path(path)?;
            Ok(GitCommand::AddFile(PathBuf::from(path)))
        }
        ["git", "add", ..] => Err("git add requires one explicit file path".to_string()),
        ["git", "commit"] => Ok(GitCommand::Commit {
            message: DBM_FIXED_COMMIT_MESSAGE.to_string(),
        }),
        ["git", "push", ..]
        | ["git", "reset", ..]
        | ["git", "clean", ..]
        | ["git", "commit", ..] => Err("dangerous git command rejected".to_string()),
        ["git", ..] => Err("unsupported git command".to_string()),
        _ => Err("not a git command".to_string()),
    }
}

pub fn validate_scoped_add_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("git add requires one explicit file path".to_string());
    }
    if path == "." {
        return Err("git add . is rejected".to_string());
    }
    if path.split_whitespace().count() != 1 {
        return Err("git add requires exactly one file path".to_string());
    }
    if path.contains('*') || path.contains('?') || path.contains('[') {
        return Err("git add rejects glob patterns".to_string());
    }
    let parsed = std::path::Path::new(path);
    if parsed.is_absolute() {
        return Err("absolute paths are rejected".to_string());
    }
    if parsed
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("parent directory paths are rejected".to_string());
    }
    Ok(())
}
