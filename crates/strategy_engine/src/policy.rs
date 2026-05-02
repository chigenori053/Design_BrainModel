/// Controls the adaptive execution strategy.
///
/// Spec §7 Strategy Policy
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyPolicy {
    /// Maximum number of retry attempts (including the initial run).
    /// Spec §7.2 default: 3
    pub max_retries: u8,
    /// Beam width for candidate generation in exploration mode.
    /// Spec §7.2 default: 3
    pub beam_width: usize,
    /// Whether the planner may attempt local IR repair.
    /// Spec §7.2 default: true
    pub allow_repair: bool,
    /// Whether the planner may generate a completely new plan.
    /// Spec §7.2 default: true
    pub allow_replan: bool,
    /// Overall timeout across all attempts (ms).  `0` = no timeout.
    pub timeout_ms: u64,
    /// When `true`, strategy selection is deterministic (same input → same choice).
    /// Spec §13 Determinismモード
    pub deterministic: bool,
}

impl Default for StrategyPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            beam_width: 3,
            allow_repair: true,
            allow_replan: true,
            timeout_ms: 0,
            deterministic: true,
        }
    }
}

impl StrategyPolicy {
    /// A minimal policy for testing: single attempt, no repair/replan.
    pub fn single_shot() -> Self {
        Self {
            max_retries: 1,
            beam_width: 1,
            allow_repair: false,
            allow_replan: false,
            timeout_ms: 0,
            deterministic: true,
        }
    }

    /// A strict policy: retry once, no replan, deterministic.
    pub fn conservative() -> Self {
        Self {
            max_retries: 2,
            beam_width: 1,
            allow_repair: true,
            allow_replan: false,
            timeout_ms: 0,
            deterministic: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let p = StrategyPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.beam_width, 3);
        assert!(p.allow_repair);
        assert!(p.allow_replan);
        assert!(p.deterministic);
    }

    #[test]
    fn single_shot_policy() {
        let p = StrategyPolicy::single_shot();
        assert_eq!(p.max_retries, 1);
        assert!(!p.allow_repair);
        assert!(!p.allow_replan);
    }
}
