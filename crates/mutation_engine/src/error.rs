use std::path::PathBuf;

#[derive(Debug)]
pub enum MutationError {
    InvalidState(&'static str),
    Rejected(Vec<String>),
    ConfirmationRequired,
    InvalidConfirmation,
    UnsafePath(PathBuf),
    MissingRecord(String),
    DriftDetected(PathBuf),
    Io(std::io::Error),
    Serialization(serde_json::Error),
    VerificationFailed(Vec<String>),
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState(message) => write!(f, "{message}"),
            Self::Rejected(violations) => write!(f, "mutation rejected: {}", violations.join("; ")),
            Self::ConfirmationRequired => write!(f, "explicit confirmation is required"),
            Self::InvalidConfirmation => write!(f, "confirmation does not match mutation"),
            Self::UnsafePath(path) => write!(f, "path is outside workspace: {}", path.display()),
            Self::MissingRecord(id) => write!(f, "mutation record not found: {id}"),
            Self::DriftDetected(path) => {
                write!(f, "workspace drift detected at {}", path.display())
            }
            Self::Io(error) => write!(f, "{error}"),
            Self::Serialization(error) => write!(f, "{error}"),
            Self::VerificationFailed(checks) => {
                write!(f, "verification failed: {}", checks.join("; "))
            }
        }
    }
}

impl std::error::Error for MutationError {}

impl From<std::io::Error> for MutationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for MutationError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}
