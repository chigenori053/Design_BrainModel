#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutPolicy {
    pub dependency_timeout_ms: u64,
    pub build_timeout_ms: u64,
    pub run_timeout_ms: u64,
    pub test_timeout_ms: u64,
}

impl Default for TimeoutPolicy {
    fn default() -> Self {
        Self {
            dependency_timeout_ms: 5_000,
            build_timeout_ms: 5_000,
            run_timeout_ms: 5_000,
            test_timeout_ms: 5_000,
        }
    }
}
