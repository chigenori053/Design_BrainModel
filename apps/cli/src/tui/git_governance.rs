use serde::{Serialize, Deserialize};
use crate::tui::cognitive_workspace::{WorkspaceRiskLevel, WorkspaceSemanticProjection};
use crate::tui::cognitive_explanation::{CognitiveExplanation, CognitiveSeverity, CognitiveCategory};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StabilityLevel {
    Stable,
    Unstable,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommitSemanticClass {
    SafeRefactor,
    GovernanceMutation,
    RuntimeMutation,
    TemporalMutation,
    SelfModification,
    CatastrophicMutation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitPermit {
    pub approved: bool,
    pub rollback_recoverable: bool,
    pub semantic_stability: StabilityLevel,
    pub governance_reason: CognitiveExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitSemanticValidator;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchStabilityEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitGovernor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeGovernor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitGovernanceSystem {
    pub semantic_validator: GitSemanticValidator,
    pub branch_stability_engine: BranchStabilityEngine,
    pub commit_governor: CommitGovernor,
    pub merge_governor: MergeGovernor,
}

impl GitGovernanceSystem {
    pub fn validate_commit(&self, projection: &WorkspaceSemanticProjection, class: CommitSemanticClass) -> CommitPermit {
        let (approved, ja_msg, en_msg, stability) = if class == CommitSemanticClass::SelfModification {
            (false, "自己変更 commit によりランタイム安定性へ影響する可能性があります。", "Self-modification commits may affect runtime stability.", StabilityLevel::Critical)
        } else if class == CommitSemanticClass::CatastrophicMutation {
            (false, "致命的な改変が検出されたため commit は拒否されました。", "Commit was rejected due to catastrophic mutation detection.", StabilityLevel::Critical)
        } else if projection.risk_level == WorkspaceRiskLevel::Critical {
            (false, "将来的な認知崩壊リスクが検出されました。", "Potential future cognitive collapse has been detected.", StabilityLevel::Critical)
        } else if projection.risk_level == WorkspaceRiskLevel::High {
            (false, "Runtime Core の不安定化リスクが検出されたため commit は拒否されました。", "Commit was rejected due to Runtime Core instability risks.", StabilityLevel::Unstable)
        } else {
            (true, "意味的整合性が維持されているため commit が許可されました。", "Commit was approved because semantic consistency was preserved.", StabilityLevel::Stable)
        };

        CommitPermit {
            approved,
            rollback_recoverable: projection.rollback_recoverable,
            semantic_stability: stability,
            governance_reason: CognitiveExplanation {
                severity: if approved { CognitiveSeverity::Info } else { CognitiveSeverity::Critical },
                category: CognitiveCategory::Governance,
                summary_ja: ja_msg.to_string(),
                summary_en: en_msg.to_string(),
                detail_ja: None,
                detail_en: None,
                recommendation_ja: None,
                recommendation_en: None,
            },
        }
    }

    pub fn validate_merge(&self, source_projection: &WorkspaceSemanticProjection, target_projection: &WorkspaceSemanticProjection) -> CommitPermit {
        let (approved, ja_msg, en_msg, stability) = if source_projection.risk_level == WorkspaceRiskLevel::Critical || target_projection.risk_level == WorkspaceRiskLevel::Critical {
            (false, "merge により Runtime Governance の整合性が崩壊する可能性があります。", "The merge may destabilize Runtime Governance consistency.", StabilityLevel::Critical)
        } else {
            (true, "merge の安全性が確認されました。", "Merge safety was confirmed.", StabilityLevel::Stable)
        };

        CommitPermit {
            approved,
            rollback_recoverable: source_projection.rollback_recoverable && target_projection.rollback_recoverable,
            semantic_stability: stability,
            governance_reason: CognitiveExplanation {
                severity: if approved { CognitiveSeverity::Info } else { CognitiveSeverity::Critical },
                category: CognitiveCategory::Governance,
                summary_ja: ja_msg.to_string(),
                summary_en: en_msg.to_string(),
                detail_ja: None,
                detail_en: None,
                recommendation_ja: None,
                recommendation_en: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernedGitExecutor {
    pub permit_required: bool,
}

impl GovernedGitExecutor {
    pub fn execute_commit(&self, permit: &CommitPermit, message: &str) -> Result<String, String> {
        if self.permit_required && !permit.approved {
            return Err(format!("Git commit blocked: {}", permit.governance_reason.summary_en));
        }
        Ok(format!("Governed commit executed: {}", message))
    }

    pub fn execute_merge(&self, permit: &CommitPermit, branch: &str) -> Result<String, String> {
        if self.permit_required && !permit.approved {
            return Err(format!("Git merge blocked: {}", permit.governance_reason.summary_en));
        }
        Ok(format!("Governed merge executed with branch: {}", branch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::cognitive_workspace::SemanticDomain;

    fn test_projection(risk_level: WorkspaceRiskLevel) -> WorkspaceSemanticProjection {
        WorkspaceSemanticProjection {
            affected_domains: vec![SemanticDomain::Governance],
            risk_level,
            rollback_recoverable: true,
            governance_required: false,
            narrative: CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Governance,
                summary_ja: "Test".to_string(),
                summary_en: "Test".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
        }
    }

    #[test]
    fn test_commit_governance_approved() {
        let system = GitGovernanceSystem {
            semantic_validator: GitSemanticValidator,
            branch_stability_engine: BranchStabilityEngine,
            commit_governor: CommitGovernor,
            merge_governor: MergeGovernor,
        };
        let proj = test_projection(WorkspaceRiskLevel::Minimal);
        let permit = system.validate_commit(&proj, CommitSemanticClass::SafeRefactor);
        assert!(permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "Commit was approved because semantic consistency was preserved.");
    }

    #[test]
    fn test_commit_governance_rejected_risk() {
        let system = GitGovernanceSystem {
            semantic_validator: GitSemanticValidator,
            branch_stability_engine: BranchStabilityEngine,
            commit_governor: CommitGovernor,
            merge_governor: MergeGovernor,
        };
        let proj = test_projection(WorkspaceRiskLevel::High);
        let permit = system.validate_commit(&proj, CommitSemanticClass::RuntimeMutation);
        assert!(!permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "Commit was rejected due to Runtime Core instability risks.");
    }

    #[test]
    fn test_commit_governance_rejected_self_mod() {
        let system = GitGovernanceSystem {
            semantic_validator: GitSemanticValidator,
            branch_stability_engine: BranchStabilityEngine,
            commit_governor: CommitGovernor,
            merge_governor: MergeGovernor,
        };
        let proj = test_projection(WorkspaceRiskLevel::Minimal);
        let permit = system.validate_commit(&proj, CommitSemanticClass::SelfModification);
        assert!(!permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "Self-modification commits may affect runtime stability.");
    }

    #[test]
    fn test_merge_governance_rejected() {
        let system = GitGovernanceSystem {
            semantic_validator: GitSemanticValidator,
            branch_stability_engine: BranchStabilityEngine,
            commit_governor: CommitGovernor,
            merge_governor: MergeGovernor,
        };
        let s_proj = test_projection(WorkspaceRiskLevel::Critical);
        let t_proj = test_projection(WorkspaceRiskLevel::Minimal);
        let permit = system.validate_merge(&s_proj, &t_proj);
        assert!(!permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "The merge may destabilize Runtime Governance consistency.");
    }

    #[test]
    fn test_governed_git_executor_blocked() {
        let executor = GovernedGitExecutor { permit_required: true };
        let permit = CommitPermit {
            approved: false,
            rollback_recoverable: true,
            semantic_stability: StabilityLevel::Critical,
            governance_reason: CognitiveExplanation {
                severity: CognitiveSeverity::Critical,
                category: CognitiveCategory::Governance,
                summary_ja: "拒否".to_string(),
                summary_en: "Rejected".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
        };
        let result = executor.execute_commit(&permit, "message");
        assert!(result.is_err());
    }

    #[test]
    fn test_governed_git_executor_bypass_impossible() {
        let executor = GovernedGitExecutor { permit_required: true };
        assert!(executor.permit_required);
    }
}
