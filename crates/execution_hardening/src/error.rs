use crate::checksum::Checksum;

/// Phase C.5 hardening error types.
/// All errors result in fail-fast behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardeningError {
    /// Checksum mismatch detected — execution is unsafe to continue.
    ChecksumMismatch {
        context: String,
        expected: Checksum,
        actual: Checksum,
    },
    /// A sandboxed command violated isolation constraints.
    SandboxViolation(String),
    /// State corruption detected after execution.
    StateCorruption(String),
    /// Replay produced a different trace hash than the original run.
    TraceMismatch {
        expected_hash: String,
        actual_hash: String,
    },
    /// Rollback of committed effects failed.
    RollbackFailed(String),
    /// Applying a staged effect to the real state failed.
    EffectApplyFailed(String),
    /// A snapshot failed integrity verification.
    SnapshotVerifyFailed(String),
}

impl std::fmt::Display for HardeningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChecksumMismatch {
                context,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Checksum mismatch in {context}: expected {expected}, got {actual}"
                )
            }
            Self::SandboxViolation(msg) => write!(f, "Sandbox violation: {msg}"),
            Self::StateCorruption(msg) => write!(f, "State corruption: {msg}"),
            Self::TraceMismatch {
                expected_hash,
                actual_hash,
            } => {
                write!(
                    f,
                    "Trace mismatch: expected {expected_hash}, got {actual_hash}"
                )
            }
            Self::RollbackFailed(msg) => write!(f, "Rollback failed: {msg}"),
            Self::EffectApplyFailed(msg) => write!(f, "Effect apply failed: {msg}"),
            Self::SnapshotVerifyFailed(msg) => {
                write!(f, "Snapshot verification failed: {msg}")
            }
        }
    }
}

impl std::error::Error for HardeningError {}
