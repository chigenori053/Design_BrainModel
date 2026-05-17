use crate::runtime::autonomous_control::RiskLevel;
use crate::runtime::cognitive_orchestration::BranchEvaluation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchBudget {
    pub max_active_branches: usize,
    pub max_speculative_branches: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchEntropyScore {
    pub entropy: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizedBranch {
    pub evaluation: BranchEvaluation,
    pub frozen: bool,
    pub pruned: bool,
}

pub fn branch_entropy(branches: &[BranchEvaluation]) -> BranchEntropyScore {
    if branches.is_empty() {
        return BranchEntropyScore { entropy: 0.0 };
    }
    let total = branches
        .iter()
        .map(|branch| branch.convergence_score.max(0.0))
        .sum::<f64>();
    if total == 0.0 {
        return BranchEntropyScore { entropy: 1.0 };
    }
    let entropy = branches
        .iter()
        .map(|branch| branch.convergence_score.max(0.0) / total)
        .filter(|probability| *probability > 0.0)
        .map(|probability| -probability * probability.log2())
        .sum::<f64>();
    BranchEntropyScore {
        entropy: entropy.min(1.0),
    }
}

pub fn optimize_branch_budget(
    branches: &[BranchEvaluation],
    budget: &BranchBudget,
) -> Vec<OptimizedBranch> {
    let mut ordered = branches.to_vec();
    ordered.sort_by(|a, b| {
        branch_rank(b)
            .cmp(&branch_rank(a))
            .then_with(|| a.branch_id.cmp(&b.branch_id))
    });
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, evaluation)| {
            let pruned =
                index >= budget.max_active_branches || evaluation.projected_risk >= RiskLevel::High;
            let frozen = !pruned && index >= budget.max_speculative_branches;
            OptimizedBranch {
                evaluation,
                frozen,
                pruned,
            }
        })
        .collect()
}

pub struct BranchInvariantSuite;

impl BranchInvariantSuite {
    pub fn assert_pruned_cannot_mutate(branch: &OptimizedBranch) {
        assert!(branch.pruned || !branch.frozen);
    }

    pub fn assert_budget_respected(branches: &[OptimizedBranch], budget: &BranchBudget) {
        let active = branches.iter().filter(|branch| !branch.pruned).count();
        assert!(active <= budget.max_active_branches);
    }
}

fn branch_rank(branch: &BranchEvaluation) -> (i64, i64, std::cmp::Reverse<RiskLevel>) {
    (
        score_key(branch.convergence_score),
        score_key(branch.semantic_score),
        std::cmp::Reverse(branch.projected_risk),
    )
}

fn score_key(value: f64) -> i64 {
    (value.clamp(0.0, 1.0) * 1_000_000.0).round() as i64
}
