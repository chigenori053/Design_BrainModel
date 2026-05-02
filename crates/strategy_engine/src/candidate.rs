use crate::types::CodeIrProgram;

// ── StrategyKind ──────────────────────────────────────────────────────────────

/// The kind of strategy being proposed.  Spec §8.2 分岐種類.
///
/// `GitAdd` and `GitCommit` are pipeline-integration variants added by
/// DBM-UX-GIT-PIPELINE-SPEC v1.0 §8.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyKind {
    /// Re-run the exact same plan without changes.
    Retry,
    /// Apply a local modification to the failing step.
    Repair,
    /// Generate an entirely new plan from the intent.
    Replan,
    /// Stop all attempts and return best-effort fallback.
    Abort,
    /// Stage a file for the next commit (pipeline §3.4).
    GitAdd,
    /// Create a git commit with the staged changes (pipeline §3.5).
    GitCommit,
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry => f.write_str("retry"),
            Self::Repair => f.write_str("repair"),
            Self::Replan => f.write_str("replan"),
            Self::Abort => f.write_str("abort"),
            Self::GitAdd => f.write_str("git_add"),
            Self::GitCommit => f.write_str("git_commit"),
        }
    }
}

// ── StrategyCandidate ─────────────────────────────────────────────────────────

/// A candidate plan with a scoring estimate.  Spec §9.2 StrategyCandidate.
///
/// Score formula (spec §9.1): `score = expected_gain - risk - cost`
#[derive(Debug, Clone)]
pub struct StrategyCandidate {
    /// The proposed plan to execute.
    pub plan: CodeIrProgram,
    /// Estimated probability of success (0.0 – 1.0).
    pub expected_gain: f32,
    /// Estimated risk of causing worse state (0.0 – 1.0).
    pub risk: f32,
    /// Estimated cost to execute (0.0 – 1.0, relative).
    pub cost: f32,
    /// What kind of strategy this candidate represents.
    pub strategy_kind: StrategyKind,
    /// Human-readable rationale for this candidate.
    pub rationale: String,
}

impl StrategyCandidate {
    /// Compute the selection score.  Spec §9.1.
    pub fn score(&self) -> f32 {
        self.expected_gain - self.risk - self.cost
    }

    // ── Factory helpers ───────────────────────────────────────────────────────

    pub fn retry(plan: CodeIrProgram) -> Self {
        Self {
            plan,
            expected_gain: 0.5,
            risk: 0.1,
            cost: 0.1,
            strategy_kind: StrategyKind::Retry,
            rationale: "Retry with identical plan — transient failures may resolve".to_string(),
        }
    }

    pub fn repair(plan: CodeIrProgram, rationale: impl Into<String>) -> Self {
        Self {
            plan,
            expected_gain: 0.7,
            risk: 0.15,
            cost: 0.15,
            strategy_kind: StrategyKind::Repair,
            rationale: rationale.into(),
        }
    }

    pub fn replan(plan: CodeIrProgram) -> Self {
        Self {
            plan,
            expected_gain: 0.6,
            risk: 0.3,
            cost: 0.5,
            strategy_kind: StrategyKind::Replan,
            rationale: "Full replan — prior plan exhausted all repair options".to_string(),
        }
    }

    /// Propose staging a file as the next pipeline step.
    ///
    /// Pipeline spec §3.4 GitAdd: Applied → Staged
    pub fn git_add(plan: CodeIrProgram, path: impl Into<String>) -> Self {
        Self {
            plan,
            expected_gain: 0.8,
            risk: 0.05,
            cost: 0.05,
            strategy_kind: StrategyKind::GitAdd,
            rationale: format!("Stage '{}' for commit", path.into()),
        }
    }

    /// Propose committing the staged changes as the next pipeline step.
    ///
    /// Pipeline spec §3.5 Commit: Staged → Committed
    pub fn git_commit(plan: CodeIrProgram) -> Self {
        Self {
            plan,
            expected_gain: 0.9,
            risk: 0.1,
            cost: 0.05,
            strategy_kind: StrategyKind::GitCommit,
            rationale: "Commit staged changes to complete the pipeline".to_string(),
        }
    }

    pub fn abort() -> Self {
        // plan field is unused for abort, but we need a value; use a minimal placeholder.
        use execution_core::engine::execution_plan::*;
        use std::path::PathBuf;
        Self {
            plan: ExecutionPlan {
                language: TargetLanguage::Rust,
                framework: None,
                project_root: PathBuf::from("/"),
                dependency_plan: DependencyPlan {
                    manifest_file: String::new(),
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
            },
            expected_gain: 0.0,
            risk: 0.0,
            cost: 0.0,
            strategy_kind: StrategyKind::Abort,
            rationale: "Abort — safety violation detected or all strategies exhausted".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use execution_core::engine::execution_plan::*;
    use std::path::PathBuf;

    fn dummy_plan() -> CodeIrProgram {
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
    fn score_formula() {
        let c = StrategyCandidate {
            plan: dummy_plan(),
            expected_gain: 0.8,
            risk: 0.1,
            cost: 0.2,
            strategy_kind: StrategyKind::Repair,
            rationale: String::new(),
        };
        assert!((c.score() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn retry_has_lower_score_than_repair() {
        let retry = StrategyCandidate::retry(dummy_plan());
        let repair = StrategyCandidate::repair(dummy_plan(), "fix build");
        // repair expected_gain is higher
        assert!(repair.score() > retry.score());
    }
}
