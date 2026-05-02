use crate::candidate::StrategyKind;
use crate::failure::FailureContext;
use execution_hardening::Checksum;

// ── StrategyOutcome ───────────────────────────────────────────────────────────

/// Final outcome of a complete strategy engine run.  Spec §12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyOutcome {
    /// At least one attempt succeeded.
    Success,
    /// All attempts failed; returned best-effort fallback result.
    /// Spec §12: fallback — failure集約 + best-effort結果返却.
    Fallback { reason: String },
    /// Execution was aborted due to a safety violation.
    /// Spec §8.2: Abort — 停止.
    Aborted { reason: String },
}

impl std::fmt::Display for StrategyOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => f.write_str("success"),
            Self::Fallback { reason } => write!(f, "fallback({reason})"),
            Self::Aborted { reason } => write!(f, "aborted({reason})"),
        }
    }
}

// ── StrategyAttempt ───────────────────────────────────────────────────────────

/// Record of a single attempt within a strategy engine run.
#[derive(Debug, Clone)]
pub struct StrategyAttempt {
    /// Zero-based index of this attempt.
    pub attempt_index: usize,
    /// What strategy was used for this attempt.
    pub strategy_kind: StrategyKind,
    /// Blake3 checksum of the plan used in this attempt.
    pub plan_checksum: Checksum,
    /// Whether this attempt succeeded.
    pub success: bool,
    /// Failure context if this attempt failed.
    pub failure_context: Option<FailureContext>,
    /// UNIX epoch ms when this attempt started.
    pub timestamp_ms: u64,
    /// Combined stdout captured from this attempt.
    pub stdout: String,
    /// Combined stderr captured from this attempt.
    pub stderr: String,
}

// ── StrategyTrace ─────────────────────────────────────────────────────────────

/// Complete audit trail of a strategy engine run.
///
/// Satisfies spec §15: traceで戦略追跡可能.
/// Each attempt is recorded with its strategy kind, plan checksum, and outcome,
/// making the entire decision sequence reproducible and auditable.
#[derive(Debug, Clone)]
pub struct StrategyTrace {
    /// Description of the original intent.
    pub intent_description: String,
    /// All attempts made, in chronological order.
    pub attempts: Vec<StrategyAttempt>,
    /// The final outcome of the strategy run.
    pub final_outcome: StrategyOutcome,
}

impl StrategyTrace {
    pub fn new(intent_description: impl Into<String>) -> Self {
        Self {
            intent_description: intent_description.into(),
            attempts: Vec::new(),
            final_outcome: StrategyOutcome::Fallback {
                reason: "not yet completed".to_string(),
            },
        }
    }

    /// Append a new attempt record.
    pub fn record(
        &mut self,
        attempt_index: usize,
        strategy_kind: StrategyKind,
        plan_checksum: Checksum,
        success: bool,
        failure_context: Option<FailureContext>,
        timestamp_ms: u64,
        stdout: String,
        stderr: String,
    ) {
        self.attempts.push(StrategyAttempt {
            attempt_index,
            strategy_kind,
            plan_checksum,
            success,
            failure_context,
            timestamp_ms,
            stdout,
            stderr,
        });
    }

    /// Finalise the trace with the given outcome.
    pub fn finish(&mut self, outcome: StrategyOutcome) {
        self.final_outcome = outcome;
    }

    /// Number of attempts recorded.
    pub fn attempt_count(&self) -> usize {
        self.attempts.len()
    }

    /// Number of successful attempts.
    pub fn success_count(&self) -> usize {
        self.attempts.iter().filter(|a| a.success).count()
    }

    /// Whether the strategy run ultimately succeeded.
    pub fn succeeded(&self) -> bool {
        matches!(self.final_outcome, StrategyOutcome::Success)
    }

    /// Collect all unique strategy kinds used, in order of first appearance.
    pub fn strategies_used(&self) -> Vec<StrategyKind> {
        let mut seen: Vec<StrategyKind> = Vec::new();
        for a in &self.attempts {
            if !seen.contains(&a.strategy_kind) {
                seen.push(a.strategy_kind.clone());
            }
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::StrategyKind;

    fn dummy_cs() -> Checksum {
        Checksum::of(b"test")
    }

    #[test]
    fn record_and_finish() {
        let mut t = StrategyTrace::new("test intent");
        t.record(
            0,
            StrategyKind::Retry,
            dummy_cs(),
            false,
            None,
            0,
            String::new(),
            "err".into(),
        );
        t.record(
            1,
            StrategyKind::Repair,
            dummy_cs(),
            true,
            None,
            1,
            "ok".into(),
            String::new(),
        );
        t.finish(StrategyOutcome::Success);

        assert_eq!(t.attempt_count(), 2);
        assert_eq!(t.success_count(), 1);
        assert!(t.succeeded());
    }

    #[test]
    fn strategies_used_deduplicates() {
        let mut t = StrategyTrace::new("x");
        t.record(
            0,
            StrategyKind::Retry,
            dummy_cs(),
            false,
            None,
            0,
            String::new(),
            String::new(),
        );
        t.record(
            1,
            StrategyKind::Retry,
            dummy_cs(),
            false,
            None,
            1,
            String::new(),
            String::new(),
        );
        t.record(
            2,
            StrategyKind::Repair,
            dummy_cs(),
            true,
            None,
            2,
            String::new(),
            String::new(),
        );
        let kinds = t.strategies_used();
        assert_eq!(kinds.len(), 2);
        assert_eq!(kinds[0], StrategyKind::Retry);
        assert_eq!(kinds[1], StrategyKind::Repair);
    }
}
