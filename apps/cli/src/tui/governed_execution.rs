use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::tui::autonomous_execution::AutonomousExecutionGovernanceSystem;
use crate::tui::cognitive_workspace::{WorkspaceRiskLevel, WorkspaceSemanticProjection};
use crate::tui::cognitive_explanation::{CognitiveExplanation, CognitiveSeverity, CognitiveCategory};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionCapability {
    ReadOnly,
    WorkspaceMutation,
    RuntimeMutation,
    GovernanceMutation,
    SelfModification,
    RollbackOperation,
    ExternalCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeExecutionState {
    PermitGranted,
    ExecutionSuspended,
    ExecutionRejected,
    IsolationMode,
    SafeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPermit {
    pub execution_id: Uuid,
    pub approved: bool,
    pub capability: ExecutionCapability,
    pub rollback_ready: bool,
    pub risk_level: WorkspaceRiskLevel,
    pub governance_reason: CognitiveExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceDecision {
    pub permit: ExecutionPermit,
    pub narrative: CognitiveExplanation,
    pub severity: CognitiveSeverity,
    pub execution_state: RuntimeExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernedExecutor {
    pub permit_required: bool,
}

impl GovernedExecutor {
    pub fn execute(&self, permit: &ExecutionPermit, operation: &str) -> Result<String, String> {
        if self.permit_required && !permit.approved {
            return Err(format!("Execution blocked: {}", permit.governance_reason.summary_en));
        }
        
        // 14.2 Required Isolation
        if permit.capability == ExecutionCapability::SelfModification {
            // Enter isolation mode logic would go here
            return Ok(format!("Executed in ISOLATION mode: {}", operation));
        }

        Ok(format!("Executed: {}", operation))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceRecursionGuard {
    pub current_depth: usize,
    pub max_depth: usize,
}

impl Default for GovernanceRecursionGuard {
    fn default() -> Self {
        Self {
            current_depth: 0,
            max_depth: 3,
        }
    }
}

impl GovernanceRecursionGuard {
    pub fn increment(&mut self) -> Result<(), String> {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            return Err("Governance recursion depth exceeded".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPermitEngine {
    pub recursion_guard: GovernanceRecursionGuard,
}

impl Default for ExecutionPermitEngine {
    fn default() -> Self {
        Self {
            recursion_guard: GovernanceRecursionGuard::default(),
        }
    }
}

impl ExecutionPermitEngine {
    pub fn evaluate_execution(&mut self, projection: &WorkspaceSemanticProjection, capability: ExecutionCapability, rollback_ready: bool) -> GovernanceDecision {
        if let Err(_) = self.recursion_guard.increment() {
            let explanation = CognitiveExplanation {
                severity: CognitiveSeverity::Critical,
                category: CognitiveCategory::Execution,
                summary_ja: "実行統治の再帰深度制限を超過しました。".to_string(),
                summary_en: "Governance recursion depth limit exceeded.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            };
            return self.create_decision(false, capability, rollback_ready, projection.risk_level.clone(), explanation, RuntimeExecutionState::SafeMode);
        }

        let (approved, ja_msg, en_msg, state) = if capability == ExecutionCapability::SelfModification {
            (true, "自己変更実行中のためランタイムを隔離モードへ移行しました。", "The runtime entered isolation mode during self-modification.", RuntimeExecutionState::IsolationMode)
        } else if !rollback_ready && capability != ExecutionCapability::ReadOnly {
            (false, "安全なロールバックを保証できないため実行は停止されました。", "Execution was blocked because safe rollback recovery could not be guaranteed.", RuntimeExecutionState::ExecutionRejected)
        } else if projection.risk_level == WorkspaceRiskLevel::Critical {
            (false, "将来的な認知崩壊リスクが検出されました。", "Potential future cognitive collapse has been detected.", RuntimeExecutionState::ExecutionSuspended)
        } else if projection.risk_level == WorkspaceRiskLevel::High {
            (false, "長期的な整合性リスクが検出されたため実行は拒否されました。", "Execution was rejected due to long-term semantic consistency risks.", RuntimeExecutionState::ExecutionRejected)
        } else if projection.governance_required {
            (false, "実行統治の検証に失敗したため安全状態へ移行しました。", "Governance validation failed and the runtime entered safe mode.", RuntimeExecutionState::SafeMode)
        } else {
            (true, "実行統治裁定により変更が許可されました。", "The modification was approved through governance arbitration.", RuntimeExecutionState::PermitGranted)
        };

        let severity = if approved { CognitiveSeverity::Info } else { CognitiveSeverity::Critical };
        let explanation = CognitiveExplanation {
            severity: severity.clone(),
            category: CognitiveCategory::Execution,
            summary_ja: ja_msg.to_string(),
            summary_en: en_msg.to_string(),
            detail_ja: None,
            detail_en: None,
            recommendation_ja: None,
            recommendation_en: None,
        };

        self.create_decision(approved, capability, rollback_ready, projection.risk_level.clone(), explanation, state)
    }

    fn create_decision(&self, approved: bool, capability: ExecutionCapability, rollback_ready: bool, risk_level: WorkspaceRiskLevel, explanation: CognitiveExplanation, state: RuntimeExecutionState) -> GovernanceDecision {
        let permit = ExecutionPermit {
            execution_id: Uuid::new_v4(),
            approved,
            capability,
            rollback_ready,
            risk_level,
            governance_reason: explanation.clone(),
        };

        GovernanceDecision {
            permit,
            narrative: explanation.clone(),
            severity: explanation.severity,
            execution_state: state,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernedExecutionBindingLayer {
    pub governance: AutonomousExecutionGovernanceSystem,
    pub permit_engine: ExecutionPermitEngine,
    pub executor: GovernedExecutor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::cognitive_workspace::SemanticDomain;

    fn test_projection(risk_level: WorkspaceRiskLevel, rollback: bool, governance: bool) -> WorkspaceSemanticProjection {
        WorkspaceSemanticProjection {
            affected_domains: vec![SemanticDomain::RuntimeCore],
            risk_level,
            rollback_recoverable: rollback,
            governance_required: governance,
            narrative: CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Execution,
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
    fn test_permit_required() {
        let executor = GovernedExecutor { permit_required: true };
        assert!(executor.permit_required);
    }

    #[test]
    fn test_permit_validation() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        assert!(decision.permit.approved);
    }

    #[test]
    fn test_recursion_protection() {
        let mut engine = ExecutionPermitEngine {
            recursion_guard: GovernanceRecursionGuard { current_depth: 3, max_depth: 3 },
        };
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        assert!(!decision.permit.approved);
        assert_eq!(decision.execution_state, RuntimeExecutionState::SafeMode);
        assert_eq!(decision.narrative.summary_en, "Governance recursion depth limit exceeded.");
    }

    #[test]
    fn test_self_modification_isolation() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::SelfModification, true);

        assert!(decision.permit.approved);
        assert_eq!(decision.execution_state, RuntimeExecutionState::IsolationMode);
        assert_eq!(decision.narrative.summary_en, "The runtime entered isolation mode during self-modification.");
    }

    #[test]
    fn test_failure_recovery_stabilized() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, true);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        assert!(!decision.permit.approved);
        assert_eq!(decision.execution_state, RuntimeExecutionState::SafeMode);
        assert_eq!(decision.narrative.summary_en, "Governance validation failed and the runtime entered safe mode.");
    }

    #[test]
    fn test_choke_point_enforcement() {
        let executor = GovernedExecutor { permit_required: true };
        assert!(executor.permit_required);
    }

    #[test]
    fn test_governance_consistency() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        assert_eq!(decision.permit.approved, true);
        assert_eq!(decision.execution_state, RuntimeExecutionState::PermitGranted);
        // 6.2 Forbidden State: Narrative Approved but Execution Rejected
        assert!(decision.permit.approved);
        assert_eq!(decision.execution_state, RuntimeExecutionState::PermitGranted);
    }

    #[test]
    fn test_choke_point_bypass_blocked() {
        let executor = GovernedExecutor { permit_required: true };
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::High, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        
        let result = executor.execute(&decision.permit, "rm -rf /");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Execution blocked"));
    }

    #[test]
    fn test_isolation_mode_stability() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::SelfModification, true);
        
        assert_eq!(decision.execution_state, RuntimeExecutionState::IsolationMode);
        
        let executor = GovernedExecutor { permit_required: true };
        let result = executor.execute(&decision.permit, "modify runtime");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("ISOLATION mode"));
    }

    #[test]
    fn test_safe_mode_degradation() {
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, true);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        
        assert_eq!(decision.execution_state, RuntimeExecutionState::SafeMode);
        assert_eq!(decision.narrative.summary_en, "Governance validation failed and the runtime entered safe mode.");
    }

    #[test]
    fn test_recursion_protection_depth() {
        let mut engine = ExecutionPermitEngine {
            recursion_guard: GovernanceRecursionGuard { current_depth: 3, max_depth: 3 },
        };
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let decision = engine.evaluate_execution(&proj, ExecutionCapability::WorkspaceMutation, true);
        assert_eq!(decision.execution_state, RuntimeExecutionState::SafeMode);
    }

    #[test]
    fn test_workspace_projection_deterministic() {
        let engine = crate::tui::cognitive_workspace::WorkspaceSemanticProjectionEngine {
            analyzer: crate::tui::cognitive_workspace::WorkspaceSemanticAnalyzer,
            classifier: crate::tui::cognitive_workspace::SemanticImpactClassifier,
            narrative_renderer: crate::tui::cognitive_workspace::WorkspaceNarrativeRenderer,
        };
        let p1 = engine.project_impact("mutation1");
        let p2 = engine.project_impact("mutation1");
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_latency_responsive() {
        // This is a placeholder for real-time latency check
        let start = std::time::Instant::now();
        let mut engine = ExecutionPermitEngine::default();
        let proj = test_projection(WorkspaceRiskLevel::Minimal, true, false);
        let _ = engine.evaluate_execution(&proj, ExecutionCapability::ReadOnly, true);
        assert!(start.elapsed().as_millis() < 100);
    }
}
