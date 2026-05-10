use serde::{Serialize, Deserialize};
use crate::tui::cognitive_explanation::{CognitiveExplanation, CognitiveSeverity, CognitiveCategory};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthorityDomain {
    LocalExecution,
    GitMutation,
    RemoteExecution,
    Deployment,
    CredentialAccess,
    SelfModification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnifiedRiskLevel {
    Minimal,
    Moderate,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TemporalStability {
    Stable,
    Unstable,
    Decaying,
    Collapsing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnifiedProtectionState {
    Normal,
    SafeMode,
    IsolationMode,
    CredentialIsolation,
    CatastrophicProtection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedGovernanceDecision {
    pub authority_domains: Vec<AuthorityDomain>,
    pub semantic_risk: UnifiedRiskLevel,
    pub rollback_recoverability: bool,
    pub temporal_stability: TemporalStability,
    pub execution_allowed: bool,
    pub narrative: CognitiveExplanation,
    pub protection_state: UnifiedProtectionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossDomainArbitrationEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorityGraphEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedTemporalStabilityEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalCatastrophicPreventionEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossDomainGovernanceSystem {
    pub arbitration_engine: CrossDomainArbitrationEngine,
    pub authority_graph: AuthorityGraphEngine,
    pub temporal_unifier: UnifiedTemporalStabilityEngine,
    pub catastrophic_engine: GlobalCatastrophicPreventionEngine,
}

impl CrossDomainGovernanceSystem {
    pub fn arbitrate(
        &self,
        domain_risks: &[(AuthorityDomain, UnifiedRiskLevel)],
        rollback_possible: bool,
        temporal_stability: TemporalStability,
    ) -> UnifiedGovernanceDecision {
        let mut highest_risk = UnifiedRiskLevel::Minimal;
        let mut domains = Vec::new();
        
        let mut has_git = false;
        let mut has_remote = false;
        let mut has_deployment = false;
        let mut has_credential = false;
        let mut has_self_mod = false;
        let mut has_runtime_mut = false;

        for (domain, risk) in domain_risks {
            domains.push(domain.clone());
            if risk > &highest_risk {
                highest_risk = risk.clone();
            }
            match domain {
                AuthorityDomain::GitMutation => has_git = true,
                AuthorityDomain::RemoteExecution => has_remote = true,
                AuthorityDomain::Deployment => has_deployment = true,
                AuthorityDomain::CredentialAccess => has_credential = true,
                AuthorityDomain::SelfModification => has_self_mod = true,
                AuthorityDomain::LocalExecution => has_runtime_mut = true,
            }
        }

        // Cross-Domain Escalation
        let mut escalated = false;
        if has_deployment && has_credential {
            highest_risk = UnifiedRiskLevel::Critical;
            escalated = true;
        } else if has_self_mod && has_remote {
            highest_risk = UnifiedRiskLevel::Critical;
            escalated = true;
        } else if has_runtime_mut && has_deployment {
            highest_risk = UnifiedRiskLevel::Critical;
            escalated = true;
        } else if has_git && has_remote {
            if highest_risk < UnifiedRiskLevel::High {
                highest_risk = UnifiedRiskLevel::High;
                escalated = true;
            }
        }

        let (mut execution_allowed, mut protection_state, mut ja_msg, mut en_msg) = if highest_risk == UnifiedRiskLevel::Critical {
            let (j, e) = if escalated {
                ("複数領域の組み合わせにより致命的不安定化が検出されました。".to_string(), "Combined authority domains produced catastrophic instability risks.".to_string())
            } else {
                ("複数の権限領域間で整合性競合が検出されました。".to_string(), "Governance consistency conflicts were detected across authority domains.".to_string())
            };
            (false, UnifiedProtectionState::CatastrophicProtection, j, e)
        } else if highest_risk == UnifiedRiskLevel::High {
            (false, UnifiedProtectionState::SafeMode, "権限連鎖による実行影響範囲が拡大しています。".to_string(), "Execution authority scope is expanding through chained governance escalation.".to_string())
        } else if temporal_stability == TemporalStability::Decaying || temporal_stability == TemporalStability::Collapsing {
            (false, UnifiedProtectionState::SafeMode, "複数領域を跨ぐ将来的な整合性低下が検出されています。".to_string(), "Cross-domain future semantic instability has been detected.".to_string())
        } else {
            (true, UnifiedProtectionState::Normal, "権限領域間の整合性が維持されています。".to_string(), "Consistency across authority domains is preserved.".to_string())
        };

        if !rollback_possible && highest_risk > UnifiedRiskLevel::Minimal {
            execution_allowed = false;
            protection_state = UnifiedProtectionState::SafeMode;
            ja_msg = "複数領域統治裁定に失敗したため安全状態へ移行しました。".to_string();
            en_msg = "Cross-domain governance arbitration failed and the runtime entered protected mode.".to_string();
        }

        UnifiedGovernanceDecision {
            authority_domains: domains,
            semantic_risk: highest_risk,
            rollback_recoverability: rollback_possible,
            temporal_stability,
            execution_allowed,
            protection_state,
            narrative: CognitiveExplanation {
                severity: if execution_allowed { CognitiveSeverity::Info } else { CognitiveSeverity::Critical },
                category: CognitiveCategory::Governance,
                summary_ja: ja_msg,
                summary_en: en_msg,
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

    fn get_system() -> CrossDomainGovernanceSystem {
        CrossDomainGovernanceSystem {
            arbitration_engine: CrossDomainArbitrationEngine,
            authority_graph: AuthorityGraphEngine,
            temporal_unifier: UnifiedTemporalStabilityEngine,
            catastrophic_engine: GlobalCatastrophicPreventionEngine,
        }
    }

    #[test]
    fn test_highest_risk_wins() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::GitMutation, UnifiedRiskLevel::Minimal),
            (AuthorityDomain::LocalExecution, UnifiedRiskLevel::Critical),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Stable);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.semantic_risk, UnifiedRiskLevel::Critical);
        assert_eq!(decision.protection_state, UnifiedProtectionState::CatastrophicProtection);
        assert_eq!(decision.narrative.summary_en, "Governance consistency conflicts were detected across authority domains.");
    }

    #[test]
    fn test_cross_domain_escalation_deployment_credential() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::Deployment, UnifiedRiskLevel::Minimal),
            (AuthorityDomain::CredentialAccess, UnifiedRiskLevel::Minimal),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Stable);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.semantic_risk, UnifiedRiskLevel::Critical);
        assert_eq!(decision.narrative.summary_en, "Combined authority domains produced catastrophic instability risks.");
    }

    #[test]
    fn test_cross_domain_escalation_git_remote() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::GitMutation, UnifiedRiskLevel::Minimal),
            (AuthorityDomain::RemoteExecution, UnifiedRiskLevel::Minimal),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Stable);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.semantic_risk, UnifiedRiskLevel::High);
        assert_eq!(decision.narrative.summary_en, "Execution authority scope is expanding through chained governance escalation.");
    }

    #[test]
    fn test_unified_temporal_stability() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::LocalExecution, UnifiedRiskLevel::Minimal),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Decaying);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.narrative.summary_en, "Cross-domain future semantic instability has been detected.");
    }

    #[test]
    fn test_catastrophic_protection_activation() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::SelfModification, UnifiedRiskLevel::Minimal),
            (AuthorityDomain::RemoteExecution, UnifiedRiskLevel::Minimal),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Stable);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.protection_state, UnifiedProtectionState::CatastrophicProtection);
    }

    #[test]
    fn test_rollback_impossibility_propagation() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::LocalExecution, UnifiedRiskLevel::Moderate),
        ];
        let decision = sys.arbitrate(&risks, false, TemporalStability::Stable);
        assert!(!decision.execution_allowed);
        assert_eq!(decision.protection_state, UnifiedProtectionState::SafeMode);
        assert_eq!(decision.narrative.summary_en, "Cross-domain governance arbitration failed and the runtime entered protected mode.");
    }

    #[test]
    fn test_unified_narrative_coherence() {
        let sys = get_system();
        let risks = vec![
            (AuthorityDomain::LocalExecution, UnifiedRiskLevel::Minimal),
        ];
        let decision = sys.arbitrate(&risks, true, TemporalStability::Stable);
        assert!(decision.execution_allowed);
        assert_eq!(decision.narrative.summary_en, "Consistency across authority domains is preserved.");
    }
}
