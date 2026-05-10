use serde::{Serialize, Deserialize};
use crate::tui::cognitive_explanation::{CognitiveExplanation, CognitiveSeverity, CognitiveCategory};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthorityScope {
    ReadOnly,
    RemoteRead,
    RemoteWrite,
    Deployment,
    InfrastructureMutation,
    CredentialAccess,
    AutonomousRemoteMutation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteRiskLevel {
    Minimal,
    Moderate,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemotePermit {
    pub approved: bool,
    pub authority_scope: AuthorityScope,
    pub remote_risk: RemoteRiskLevel,
    pub rollback_possible: bool,
    pub governance_reason: CognitiveExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorityValidationEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteRiskEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialGovernor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalExecutionGovernor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteGovernanceSystem {
    pub authority_engine: AuthorityValidationEngine,
    pub remote_risk_engine: RemoteRiskEngine,
    pub credential_governor: CredentialGovernor,
    pub external_execution_governor: ExternalExecutionGovernor,
}

impl RemoteGovernanceSystem {
    pub fn evaluate_remote_operation(&self, scope: AuthorityScope, risk: RemoteRiskLevel, rollback_possible: bool) -> RemotePermit {
        let (approved, ja_msg, en_msg) = if scope == AuthorityScope::CredentialAccess {
            (false, "高権限 credential へのアクセス要求が検出されました。", "High-authority credential access was requested.")
        } else if scope == AuthorityScope::InfrastructureMutation || scope == AuthorityScope::AutonomousRemoteMutation {
            (false, "致命的な外部環境破壊リスクが検出されたため実行は停止されました。", "Remote execution was blocked due to catastrophic infrastructure mutation risks.")
        } else if scope == AuthorityScope::Deployment && risk == RemoteRiskLevel::Critical {
            (false, "deploy 後の収束性崩壊リスクが検出されたため実行は停止されました。", "Deployment was blocked due to post-deployment convergence collapse risks.")
        } else if risk == RemoteRiskLevel::Critical {
            (false, "外部統治検証に失敗したため操作は停止されました。", "Remote governance validation failed and the operation was blocked.")
        } else {
            (true, "外部実行は統治裁定によって許可されました。", "External execution was approved through governance arbitration.")
        };

        RemotePermit {
            approved,
            authority_scope: scope,
            remote_risk: risk,
            rollback_possible,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_permit_required() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::RemoteRead, RemoteRiskLevel::Minimal, true);
        assert!(permit.approved);
    }

    #[test]
    fn test_authority_scope_validation() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::InfrastructureMutation, RemoteRiskLevel::High, false);
        assert!(!permit.approved);
    }

    #[test]
    fn test_credential_isolation_enforcement() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::CredentialAccess, RemoteRiskLevel::Minimal, false);
        assert!(!permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "High-authority credential access was requested.");
    }

    #[test]
    fn test_deployment_rejection() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::Deployment, RemoteRiskLevel::Critical, true);
        assert!(!permit.approved);
        assert_eq!(permit.governance_reason.summary_en, "Deployment was blocked due to post-deployment convergence collapse risks.");
    }

    #[test]
    fn test_remote_risk_validation() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::RemoteWrite, RemoteRiskLevel::Critical, true);
        assert!(!permit.approved);
    }

    #[test]
    fn test_catastrophic_prevention() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::AutonomousRemoteMutation, RemoteRiskLevel::High, false);
        assert!(!permit.approved);
    }

    #[test]
    fn test_remote_approval_narrative() {
        let system = RemoteGovernanceSystem {
            authority_engine: AuthorityValidationEngine,
            remote_risk_engine: RemoteRiskEngine,
            credential_governor: CredentialGovernor,
            external_execution_governor: ExternalExecutionGovernor,
        };
        let permit = system.evaluate_remote_operation(AuthorityScope::RemoteRead, RemoteRiskLevel::Minimal, true);
        assert_eq!(permit.governance_reason.summary_en, "External execution was approved through governance arbitration.");
    }
}
