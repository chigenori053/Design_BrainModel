use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::tui::cognitive_explanation::{
    CognitiveCategory, CognitiveExplanation, CognitiveSeverity,
};
use crate::tui::cross_domain_governance::{
    AuthorityDomain, UnifiedGovernanceDecision, UnifiedRiskLevel,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceTimelineEvent {
    pub timestamp: SystemTime,
    pub authority_domains: Vec<AuthorityDomain>,
    pub decision: UnifiedGovernanceDecision,
    pub causal_reason: CognitiveExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceTimelineEngine {
    pub events: Vec<GovernanceTimelineEvent>,
}

impl GovernanceTimelineEngine {
    pub fn record_event(
        &mut self,
        domains: Vec<AuthorityDomain>,
        decision: UnifiedGovernanceDecision,
    ) {
        let event = GovernanceTimelineEvent {
            timestamp: SystemTime::now(),
            authority_domains: domains,
            causal_reason: decision.narrative.clone(),
            decision,
        };
        self.events.push(event);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorityTraceEngine;

impl AuthorityTraceEngine {
    pub fn trace_escalation(
        &self,
        domains: &[AuthorityDomain],
        target_risk: UnifiedRiskLevel,
    ) -> CognitiveExplanation {
        let (ja, en) = if domains.contains(&AuthorityDomain::Deployment)
            && domains.contains(&AuthorityDomain::CredentialAccess)
        {
            ("Remote deployment authority が CredentialAccessとの組み合わせにより Critical へ昇格しました。".to_string(),
             "Remote deployment authority escalated to Critical when combined with CredentialAccess.".to_string())
        } else {
            (
                format!(
                    "権限領域の組み合わせにより {:?} へ昇格しました。",
                    target_risk
                ),
                format!(
                    "Authority escalated to {:?} due to domain combination.",
                    target_risk
                ),
            )
        };

        CognitiveExplanation {
            severity: CognitiveSeverity::Critical,
            category: CognitiveCategory::Governance,
            summary_ja: ja,
            summary_en: en,
            detail_ja: None,
            detail_en: None,
            recommendation_ja: None,
            recommendation_en: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceReplayEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceCausalNarrativeEngine;

impl GovernanceCausalNarrativeEngine {
    pub fn generate_causal_narrative(
        &self,
        initial_safe: bool,
        final_risk: UnifiedRiskLevel,
    ) -> CognitiveExplanation {
        let (ja, en) = if initial_safe && final_risk >= UnifiedRiskLevel::High {
            ("当初は安全と判定されていましたが、特定の権限が追加されたことで統治リスクが上昇しました。".to_string(),
             "The operation was initially considered safe, but governance risk increased after additional authority was added.".to_string())
        } else {
            (
                "意味的整合性が維持されています。".to_string(),
                "Semantic consistency is preserved.".to_string(),
            )
        };

        CognitiveExplanation {
            severity: if final_risk >= UnifiedRiskLevel::High {
                CognitiveSeverity::Warning
            } else {
                CognitiveSeverity::Info
            },
            category: CognitiveCategory::Governance,
            summary_ja: ja,
            summary_en: en,
            detail_ja: None,
            detail_en: None,
            recommendation_ja: None,
            recommendation_en: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceMemoryEngine {
    pub history: Vec<UnifiedGovernanceDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceCompressionLayer;

impl GovernanceCompressionLayer {
    pub fn compress_narratives(&self, narratives: Vec<String>) -> String {
        if narratives.len() > 2 {
            "Cross-domain authority instability has increased.".to_string()
        } else {
            narratives.join(" ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceHeatmapEngine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceObservabilitySystem {
    pub timeline_engine: GovernanceTimelineEngine,
    pub trace_engine: AuthorityTraceEngine,
    pub replay_engine: GovernanceReplayEngine,
    pub causal_engine: GovernanceCausalNarrativeEngine,
    pub memory_engine: GovernanceMemoryEngine,
    pub compression_layer: GovernanceCompressionLayer,
}

impl GovernanceObservabilitySystem {
    pub fn new() -> Self {
        Self {
            timeline_engine: GovernanceTimelineEngine { events: Vec::new() },
            trace_engine: AuthorityTraceEngine,
            replay_engine: GovernanceReplayEngine,
            causal_engine: GovernanceCausalNarrativeEngine,
            memory_engine: GovernanceMemoryEngine {
                history: Vec::new(),
            },
            compression_layer: GovernanceCompressionLayer,
        }
    }

    pub fn process_decision(&mut self, decision: UnifiedGovernanceDecision) {
        self.timeline_engine
            .record_event(decision.authority_domains.clone(), decision.clone());
        self.memory_engine.history.push(decision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::cross_domain_governance::{TemporalStability, UnifiedProtectionState};

    fn mock_decision(risk: UnifiedRiskLevel, allowed: bool) -> UnifiedGovernanceDecision {
        UnifiedGovernanceDecision {
            authority_domains: vec![AuthorityDomain::LocalExecution],
            semantic_risk: risk,
            rollback_recoverability: true,
            temporal_stability: TemporalStability::Stable,
            execution_allowed: allowed,
            protection_state: if allowed {
                UnifiedProtectionState::Normal
            } else {
                UnifiedProtectionState::SafeMode
            },
            narrative: CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Governance,
                summary_ja: "テスト".to_string(),
                summary_en: "Test".to_string(),
                detail_ja: None,
                detail_en: None,
                recommendation_ja: None,
                recommendation_en: None,
            },
        }
    }

    #[test]
    fn test_governance_event_recording() {
        let mut sys = GovernanceObservabilitySystem::new();
        let decision = mock_decision(UnifiedRiskLevel::Minimal, true);
        sys.process_decision(decision);
        assert_eq!(sys.timeline_engine.events.len(), 1);
    }

    #[test]
    fn test_authority_escalation_tracing() {
        let engine = AuthorityTraceEngine;
        let domains = vec![
            AuthorityDomain::Deployment,
            AuthorityDomain::CredentialAccess,
        ];
        let trace = engine.trace_escalation(&domains, UnifiedRiskLevel::Critical);
        assert_eq!(
            trace.summary_en,
            "Remote deployment authority escalated to Critical when combined with CredentialAccess."
        );
    }

    #[test]
    fn test_causal_narrative_generation() {
        let engine = GovernanceCausalNarrativeEngine;
        let narrative = engine.generate_causal_narrative(true, UnifiedRiskLevel::High);
        assert!(narrative.summary_en.contains("initially considered safe"));
    }

    #[test]
    fn test_governance_compression() {
        let layer = GovernanceCompressionLayer;
        let narratives = vec![
            "Risk1".to_string(),
            "Risk2".to_string(),
            "Risk3".to_string(),
        ];
        let compressed = layer.compress_narratives(narratives);
        assert_eq!(
            compressed,
            "Cross-domain authority instability has increased."
        );
    }
}
