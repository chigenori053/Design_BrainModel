use std::path::PathBuf;

use super::state::CommitDecisionContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchSafety {
    Safe,
    Protected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitGuardDecision {
    Allow { files: Vec<PathBuf> },
    Reject { reason: &'static str },
}

pub struct CommitGuard;

impl CommitGuard {
    pub fn branch_safety(branch: &str) -> BranchSafety {
        if matches!(branch, "main" | "master") {
            BranchSafety::Protected
        } else {
            BranchSafety::Safe
        }
    }

    pub fn evaluate(context: &CommitDecisionContext) -> CommitGuardDecision {
        if Self::branch_safety(&context.branch_name) == BranchSafety::Protected {
            return CommitGuardDecision::Reject {
                reason: "protected branch conflict",
            };
        }
        if !context.explicit_confirmation {
            return CommitGuardDecision::Reject {
                reason: "explicit confirmation required",
            };
        }
        if !context.diff_preview_ready {
            return CommitGuardDecision::Reject {
                reason: "diff preview mandatory",
            };
        }
        if context.changed_files.is_empty() {
            return CommitGuardDecision::Reject {
                reason: "explicit files only",
            };
        }

        CommitGuardDecision::Allow {
            files: context.changed_files.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_branch_is_rejected() {
        let decision = CommitGuard::evaluate(&CommitDecisionContext {
            branch_name: "main".to_string(),
            changed_files: vec![PathBuf::from("apps/cli/src/nl/loop/state.rs")],
            explicit_confirmation: true,
            diff_preview_ready: true,
        });
        assert_eq!(
            decision,
            CommitGuardDecision::Reject {
                reason: "protected branch conflict"
            }
        );
    }
}
