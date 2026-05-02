use crate::candidate::{StrategyCandidate, StrategyKind};
use crate::history::ExecutionHistory;

/// Selects the best strategy candidate from a set of options.
///
/// Spec §9 StrategySelector
///
/// In **deterministic mode** (`policy.deterministic == true`): returns the
/// single highest-scoring candidate, resolving ties by strategy kind priority
/// (Repair > Retry > Replan > Abort).
///
/// In **exploration mode**: returns the top-`k` candidates for beam search.
#[derive(Debug, Default)]
pub struct StrategySelector {
    /// When `true`, selection is fully deterministic.
    pub deterministic: bool,
}

impl StrategySelector {
    pub fn new(deterministic: bool) -> Self {
        Self { deterministic }
    }

    /// Select the single best candidate.  Spec §9.3.
    ///
    /// Returns `None` if `candidates` is empty.
    pub fn select_best(&self, mut candidates: Vec<StrategyCandidate>) -> Option<StrategyCandidate> {
        if candidates.is_empty() {
            return None;
        }
        // Sort descending by score; break ties by strategy kind priority.
        candidates.sort_by(|a, b| {
            let sa = a.score();
            let sb = b.score();
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_priority(&a.strategy_kind).cmp(&kind_priority(&b.strategy_kind)))
        });
        candidates.into_iter().next()
    }

    /// Select the top-`k` candidates for beam-search exploration.  Spec §9.3.
    pub fn select_top_k(
        &self,
        mut candidates: Vec<StrategyCandidate>,
        k: usize,
    ) -> Vec<StrategyCandidate> {
        candidates.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_priority(&a.strategy_kind).cmp(&kind_priority(&b.strategy_kind)))
        });
        candidates.into_iter().take(k).collect()
    }

    // ── History-weighted variants (spec §8) ───────────────────────────────────

    /// Adjusted score: `score = base_score + history_success_weight - history_failure_weight`.
    ///
    /// Spec §8: +0.1 for plans that previously succeeded, -0.2 for plans that
    /// previously failed, ensuring the selector avoids known-bad plans.
    pub fn score_with_history(
        &self,
        candidate: &StrategyCandidate,
        history: &ExecutionHistory,
    ) -> f32 {
        let base = candidate.score();
        let success_bonus = if history.has_succeeded(&candidate.plan) {
            0.1
        } else {
            0.0
        };
        let failure_penalty = if history.has_failed(&candidate.plan) {
            0.2
        } else {
            0.0
        };
        base + success_bonus - failure_penalty
    }

    /// Select the single best candidate, adjusting scores by execution history.
    pub fn select_best_with_history(
        &self,
        mut candidates: Vec<StrategyCandidate>,
        history: &ExecutionHistory,
    ) -> Option<StrategyCandidate> {
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by(|a, b| {
            let sa = self.score_with_history(a, history);
            let sb = self.score_with_history(b, history);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_priority(&a.strategy_kind).cmp(&kind_priority(&b.strategy_kind)))
        });
        candidates.into_iter().next()
    }

    /// Select the top-k candidates in Proposal mode — no history weighting,
    /// no beam expansion.  Spec DBM-EXPLOSION-FIX-TIER1-SPEC §7.1.
    ///
    /// Equivalent to `top_k_without_expansion` in the spec.  Simply sorts by
    /// score and truncates — no recursive strategy generation occurs.
    pub fn select_for_proposal(
        &self,
        mut candidates: Vec<StrategyCandidate>,
    ) -> Vec<StrategyCandidate> {
        candidates.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_priority(&a.strategy_kind).cmp(&kind_priority(&b.strategy_kind)))
        });
        candidates.truncate(crate::proposal::MAX_CANDIDATES);
        candidates
    }

    /// Select the top-`k` candidates, adjusting scores by execution history.
    pub fn select_top_k_with_history(
        &self,
        mut candidates: Vec<StrategyCandidate>,
        k: usize,
        history: &ExecutionHistory,
    ) -> Vec<StrategyCandidate> {
        candidates.sort_by(|a, b| {
            let sa = self.score_with_history(a, history);
            let sb = self.score_with_history(b, history);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_priority(&a.strategy_kind).cmp(&kind_priority(&b.strategy_kind)))
        });
        candidates.into_iter().take(k).collect()
    }
}

/// Lower value = higher priority when scores are equal.
fn kind_priority(kind: &StrategyKind) -> u8 {
    match kind {
        StrategyKind::Repair => 0,
        StrategyKind::Retry => 1,
        StrategyKind::Replan => 2,
        StrategyKind::Abort => 3,
        // Pipeline steps: commit is the natural follow-on to add, so rank higher.
        StrategyKind::GitCommit => 4,
        StrategyKind::GitAdd => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::StrategyCandidate;
    use crate::types::CodeIrProgram;
    use execution_core::engine::execution_plan::*;
    use std::path::PathBuf;

    fn plan() -> CodeIrProgram {
        ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec![],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: TestPlan {
                test_files: vec![],
                test_commands: vec![],
            },
        }
    }

    #[test]
    fn selects_highest_score() {
        let sel = StrategySelector::new(true);
        let candidates = vec![
            StrategyCandidate {
                plan: plan(),
                expected_gain: 0.5,
                risk: 0.1,
                cost: 0.1,
                strategy_kind: StrategyKind::Retry,
                rationale: String::new(),
            },
            StrategyCandidate {
                plan: plan(),
                expected_gain: 0.9,
                risk: 0.1,
                cost: 0.1,
                strategy_kind: StrategyKind::Repair,
                rationale: String::new(),
            },
            StrategyCandidate {
                plan: plan(),
                expected_gain: 0.3,
                risk: 0.1,
                cost: 0.1,
                strategy_kind: StrategyKind::Replan,
                rationale: String::new(),
            },
        ];
        let best = sel.select_best(candidates).unwrap();
        assert_eq!(best.strategy_kind, StrategyKind::Repair);
    }

    #[test]
    fn empty_candidates_returns_none() {
        let sel = StrategySelector::new(true);
        assert!(sel.select_best(vec![]).is_none());
    }

    #[test]
    fn top_k_returns_k_items() {
        let sel = StrategySelector::new(true);
        let candidates = vec![
            StrategyCandidate::retry(plan()),
            StrategyCandidate::repair(plan(), "fix"),
            StrategyCandidate::replan(plan()),
        ];
        assert_eq!(sel.select_top_k(candidates, 2).len(), 2);
    }

    #[test]
    fn tie_broken_by_kind_priority() {
        let sel = StrategySelector::new(true);
        // Same score — Repair should win over Retry
        let retry = StrategyCandidate {
            plan: plan(),
            expected_gain: 0.5,
            risk: 0.1,
            cost: 0.1,
            strategy_kind: StrategyKind::Retry,
            rationale: String::new(),
        };
        let repair = StrategyCandidate {
            plan: plan(),
            expected_gain: 0.5,
            risk: 0.1,
            cost: 0.1,
            strategy_kind: StrategyKind::Repair,
            rationale: String::new(),
        };
        let best = sel.select_best(vec![retry, repair]).unwrap();
        assert_eq!(best.strategy_kind, StrategyKind::Repair);
    }
}
