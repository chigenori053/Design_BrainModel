#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureType {
    // ── Original failure kinds ────────────────────────────────────────────
    DependencyFailure,
    BuildFailure,
    RuntimeFailure,
    TestFailure,
    Timeout,
    EnvironmentError,

    // ── Phase C.5 hardening failure kinds (spec §10) ──────────────────────
    /// Physical execution state became inconsistent — cannot safely proceed.
    /// Spec §10.1: StateCorruptionError
    StateCorruption,

    /// A sandboxed command violated isolation constraints (forbidden binary,
    /// shell flag, pipe, or dynamic generation).
    /// Spec §10.1: SandboxViolation
    SandboxViolation,

    /// A replay run produced a different `ExecutionTraceHash` than the
    /// original execution — non-determinism detected.
    /// Spec §10.1: TraceMismatch
    TraceMismatch,

    /// A checksum computed after execution did not match the expected value.
    ChecksumMismatch,
}

impl FailureType {
    /// Returns `true` for failures that indicate the execution environment
    /// itself is compromised and no further steps should run.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::StateCorruption
                | Self::SandboxViolation
                | Self::TraceMismatch
                | Self::ChecksumMismatch
        )
    }
}
