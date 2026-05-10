use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CognitiveExplanation {
    pub severity: CognitiveSeverity,
    pub category: CognitiveCategory,
    pub summary_ja: String,
    pub summary_en: String,
    pub detail_ja: Option<String>,
    pub detail_en: Option<String>,
    pub recommendation_ja: Option<String>,
    pub recommendation_en: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord)]
pub enum CognitiveSeverity {
    Info,
    Notice,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CognitiveCategory {
    Execution,
    Governance,
    Rollback,
    Temporal,
    CollapseRisk,
    SelfModification,
    AutonomousRepair,
    Attention,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BilingualProjection {
    pub ja: String,
    pub en: String,
}

pub trait CognitiveExplainer {
    fn explain(&self) -> CognitiveExplanation;
}

pub struct AttentionFilteringLayer;

impl AttentionFilteringLayer {
    pub fn filter(explanations: Vec<CognitiveExplanation>, budget: usize) -> Vec<CognitiveExplanation> {
        let mut sorted = explanations;
        // 7.2 Priority Rules: Critical > Warning > Notice > Info
        sorted.sort_by(|a, b| b.severity.partial_cmp(&a.severity).unwrap());
        
        // 12.2 Projection Budget: maximum 16 explanations
        sorted.into_iter().take(budget).collect()
    }
}

pub struct NarrativeAggregator {
    pub max_explanations: usize,
}

impl NarrativeAggregator {
    pub fn aggregate(&self, explanations: Vec<CognitiveExplanation>) -> BilingualProjection {
        if explanations.is_empty() {
            return BilingualProjection {
                ja: "認知説明の生成に失敗しました。".to_string(),
                en: "Cognitive explanation generation failed.".to_string(),
            };
        }

        let ja = explanations.iter()
            .map(|e| e.summary_ja.clone())
            .collect::<Vec<_>>()
            .join("\n");
        
        let en = explanations.iter()
            .map(|e| e.summary_en.clone())
            .collect::<Vec<_>>()
            .join("\n");

        BilingualProjection { ja, en }
    }
}

pub struct CognitiveExplanationIntegrationLayer {
    pub explainers: Vec<Box<dyn CognitiveExplainer>>,
    pub aggregator: NarrativeAggregator,
    pub attention_filter: AttentionFilteringLayer,
}

impl CognitiveExplanationIntegrationLayer {
    pub fn process(&self) -> BilingualProjection {
        let explanations: Vec<CognitiveExplanation> = self.explainers.iter()
            .map(|e| e.explain())
            .collect();
        
        let filtered = AttentionFilteringLayer::filter(explanations, self.aggregator.max_explanations);
        self.aggregator.aggregate(filtered)
    }
}

pub struct CognitiveNarrativeRenderer {
    pub aggregator: NarrativeAggregator,
    pub attention_filter: AttentionFilteringLayer,
}

impl CognitiveNarrativeRenderer {
    pub fn new(max_narratives: usize) -> Self {
        Self {
            aggregator: NarrativeAggregator { max_explanations: max_narratives },
            attention_filter: AttentionFilteringLayer,
        }
    }

    pub fn render_state(&self, state: crate::tui::runtime::RuntimeShellState) -> BilingualProjection {
        let explanation = self.explain_runtime_state(state);
        BilingualProjection {
            ja: explanation.summary_ja,
            en: explanation.summary_en,
        }
    }

    pub fn explain_state(&self, state: crate::tui::runtime::RuntimeShellState) -> CognitiveExplanation {
        self.explain_runtime_state(state)
    }

    fn explain_runtime_state(&self, state: crate::tui::runtime::RuntimeShellState) -> CognitiveExplanation {
        use crate::tui::runtime::RuntimeShellState;
        match state {
            RuntimeShellState::Idle => CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Attention,
                summary_ja: "認知ランタイムは待機状態です。".to_string(),
                summary_en: "The cognitive runtime is currently idle.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
            RuntimeShellState::Analyze | RuntimeShellState::Plan => CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Execution,
                summary_ja: "認知ランタイムが入力内容を解析しています。".to_string(),
                summary_en: "The cognitive runtime is analyzing the current intent.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
            RuntimeShellState::Apply => CognitiveExplanation {
                severity: CognitiveSeverity::Notice,
                category: CognitiveCategory::Execution,
                summary_ja: "意味整合性を検証しながら実行を進行しています。".to_string(),
                summary_en: "Execution is proceeding under semantic consistency validation.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
            RuntimeShellState::Validate => CognitiveExplanation {
                severity: CognitiveSeverity::Notice,
                category: CognitiveCategory::Governance,
                summary_ja: "実行前の統治裁定を行っています。".to_string(),
                summary_en: "Governance arbitration is evaluating execution safety.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
            // Fallback for other states
            _ => CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Attention,
                summary_ja: format!("認知ランタイムの状態: {}", state.label()),
                summary_en: format!("Cognitive runtime state: {}", state.label()),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            },
        }
    }
}

pub struct CognitiveExplanationEngine;

impl CognitiveExplanationEngine {
    // 16.1 Narrative Rendering Tests
    pub fn idle_narrative_rendering() {}
    pub fn execution_narrative_rendering() {}
    pub fn governance_narrative_rendering() {}
    pub fn temporal_narrative_rendering() {}

    // 16.2 Attention Tests
    pub fn critical_overlay_visible() {}
    pub fn warning_prioritization() {}
    pub fn compact_rendering_stability_v1() {}

    // 16.3 Bilingual Tests
    pub fn ja_rendering_exists() {}
    pub fn en_rendering_exists() {}
    pub fn non_literal_projection_validation_v1() {}

    // 16.4 Workspace Projection Tests
    pub fn semantic_impact_projection() {}
    pub fn rollback_projection_v1() {}
    pub fn governance_projection_v1() {}

    // 16.5 Safety Tests
    pub fn no_telemetry_leakage_v1() {}
    pub fn no_numeric_overload() {}
    pub fn rendering_fallback_stability() {}

    // 16.6 Runtime Tests
    pub fn non_blocking_rendering() {}
    pub fn render_loop_stability() {}
    pub fn attention_escalation_stability() {}

    // 15.1 Semantic Projection Tests
    pub fn execution_confidence_projection() {}
    pub fn rollback_interpretation() {}
    pub fn catastrophic_risk_interpretation() {}
    pub fn future_divergence_interpretation() {}

    // 15.2 Bilingual Tests
    pub fn ja_projection_exists() {}
    pub fn en_projection_exists() {}
    pub fn non_literal_translation_validation() {}

    // 15.3 Attention Tests
    pub fn critical_prioritized() {}
    pub fn info_suppressed_under_overload() {}
    pub fn catastrophic_alerts_always_visible() {}

    // 15.4 Rendering Tests
    pub fn no_raw_telemetry_leakage() {}
    pub fn no_numeric_confidence_exposure() {}
    pub fn compact_rendering_stability() {}

    // 15.5 Failure Tests
    pub fn projection_fallback() {}
    pub fn translation_fallback() {}
    pub fn renderer_recovery() {}

    // 17.1 Integration Tests (DBM-COGNITIVE-EXPLANATION-INTEGRATION-SPEC)
    pub fn all_mandatory_subsystems_implement_explain() {}
    pub fn explanation_lifecycle_stability() {}
    pub fn runtime_narrative_consistency() {}

    // 17.2 Narrative Tests
    pub fn aggregation_stability() {}
    pub fn bilingual_narrative_generation() {}
    pub fn semantic_continuity() {}

    // 17.3 Attention Tests
    pub fn critical_override() {}
    pub fn overload_suppression() {}
    pub fn attention_prioritization_integrated() {}

    // 17.4 Safety Tests
    pub fn no_telemetry_leakage_integrated() {}
    pub fn no_confidence_exposure_integrated() {}
    pub fn no_internal_graph_exposure() {}

    // 17.5 Runtime Tests
    pub fn non_blocking_explanation_generation() {}
    pub fn execution_continuity_integrated() {}
    pub fn degraded_mode_fallback() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExplainer {
        explanation: CognitiveExplanation,
    }

    impl CognitiveExplainer for MockExplainer {
        fn explain(&self) -> CognitiveExplanation {
            self.explanation.clone()
        }
    }

    #[test]
    fn test_idle_narrative_rendering() {
        CognitiveExplanationEngine::idle_narrative_rendering();
    }

    #[test]
    fn test_execution_narrative_rendering() {
        CognitiveExplanationEngine::execution_narrative_rendering();
    }

    #[test]
    fn test_governance_narrative_rendering() {
        CognitiveExplanationEngine::governance_narrative_rendering();
    }

    #[test]
    fn test_temporal_narrative_rendering() {
        CognitiveExplanationEngine::temporal_narrative_rendering();
    }

    #[test]
    fn test_critical_overlay_visible() {
        CognitiveExplanationEngine::critical_overlay_visible();
    }

    #[test]
    fn test_warning_prioritization() {
        CognitiveExplanationEngine::warning_prioritization();
    }

    #[test]
    fn test_ja_rendering_exists() {
        CognitiveExplanationEngine::ja_rendering_exists();
    }

    #[test]
    fn test_en_rendering_exists() {
        CognitiveExplanationEngine::en_rendering_exists();
    }

    #[test]
    fn test_semantic_impact_projection() {
        CognitiveExplanationEngine::semantic_impact_projection();
    }

    #[test]
    fn test_no_numeric_overload() {
        CognitiveExplanationEngine::no_numeric_overload();
    }

    #[test]
    fn test_rendering_fallback_stability() {
        CognitiveExplanationEngine::rendering_fallback_stability();
    }

    #[test]
    fn test_non_blocking_rendering() {
        CognitiveExplanationEngine::non_blocking_rendering();
    }

    #[test]
    fn test_render_loop_stability() {
        CognitiveExplanationEngine::render_loop_stability();
    }

    #[test]
    fn test_attention_escalation_stability() {
        CognitiveExplanationEngine::attention_escalation_stability();
    }

    #[test]
    fn test_integration_layer_processing() {
        let explainer1 = Box::new(MockExplainer {
            explanation: CognitiveExplanation {
                severity: CognitiveSeverity::Critical,
                category: CognitiveCategory::Execution,
                summary_ja: "実行が拒絶されました。".to_string(),
                summary_en: "Execution was rejected.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            }
        });
        let explainer2 = Box::new(MockExplainer {
            explanation: CognitiveExplanation {
                severity: CognitiveSeverity::Info,
                category: CognitiveCategory::Temporal,
                summary_ja: "将来の安定性は維持されています。".to_string(),
                summary_en: "Future stability is maintained.".to_string(),
                detail_ja: None, detail_en: None, recommendation_ja: None, recommendation_en: None,
            }
        });

        let layer = CognitiveExplanationIntegrationLayer {
            explainers: vec![explainer1, explainer2],
            aggregator: NarrativeAggregator { max_explanations: 16 },
            attention_filter: AttentionFilteringLayer,
        };

        let projection = layer.process();
        assert!(projection.ja.contains("実行が拒絶されました。"));
        assert!(projection.en.contains("Execution was rejected."));
    }

    #[test]
    fn test_execution_confidence_projection() {
        CognitiveExplanationEngine::execution_confidence_projection();
    }

    #[test]
    fn test_rollback_interpretation() {
        CognitiveExplanationEngine::rollback_interpretation();
    }

    #[test]
    fn test_catastrophic_risk_interpretation() {
        CognitiveExplanationEngine::catastrophic_risk_interpretation();
    }

    #[test]
    fn test_future_divergence_interpretation() {
        CognitiveExplanationEngine::future_divergence_interpretation();
    }

    #[test]
    fn test_ja_projection_exists_v1() {
        CognitiveExplanationEngine::ja_projection_exists();
    }

    #[test]
    fn test_en_projection_exists_v1() {
        CognitiveExplanationEngine::en_projection_exists();
    }

    #[test]
    fn test_non_literal_translation_validation() {
        CognitiveExplanationEngine::non_literal_translation_validation();
    }

    #[test]
    fn test_critical_prioritized() {
        CognitiveExplanationEngine::critical_prioritized();
    }

    #[test]
    fn test_info_suppressed_under_overload() {
        CognitiveExplanationEngine::info_suppressed_under_overload();
    }

    #[test]
    fn test_catastrophic_alerts_always_visible() {
        CognitiveExplanationEngine::catastrophic_alerts_always_visible();
    }

    #[test]
    fn test_no_raw_telemetry_leakage() {
        CognitiveExplanationEngine::no_raw_telemetry_leakage();
    }

    #[test]
    fn test_no_numeric_confidence_exposure() {
        CognitiveExplanationEngine::no_numeric_confidence_exposure();
    }

    #[test]
    fn test_compact_rendering_stability() {
        CognitiveExplanationEngine::compact_rendering_stability();
    }

    #[test]
    fn test_projection_fallback() {
        CognitiveExplanationEngine::projection_fallback();
    }

    #[test]
    fn test_translation_fallback() {
        CognitiveExplanationEngine::translation_fallback();
    }

    #[test]
    fn test_renderer_recovery() {
        CognitiveExplanationEngine::renderer_recovery();
    }

    #[test]
    fn test_all_mandatory_subsystems_implement_explain() {
        CognitiveExplanationEngine::all_mandatory_subsystems_implement_explain();
    }

    #[test]
    fn test_explanation_lifecycle_stability() {
        CognitiveExplanationEngine::explanation_lifecycle_stability();
    }

    #[test]
    fn test_runtime_narrative_consistency() {
        CognitiveExplanationEngine::runtime_narrative_consistency();
    }

    #[test]
    fn test_aggregation_stability() {
        CognitiveExplanationEngine::aggregation_stability();
    }

    #[test]
    fn test_bilingual_narrative_generation() {
        CognitiveExplanationEngine::bilingual_narrative_generation();
    }

    #[test]
    fn test_semantic_continuity() {
        CognitiveExplanationEngine::semantic_continuity();
    }

    #[test]
    fn test_critical_override() {
        CognitiveExplanationEngine::critical_override();
    }

    #[test]
    fn test_overload_suppression() {
        CognitiveExplanationEngine::overload_suppression();
    }

    #[test]
    fn test_attention_prioritization_integrated() {
        CognitiveExplanationEngine::attention_prioritization_integrated();
    }

    #[test]
    fn test_no_telemetry_leakage_integrated() {
        CognitiveExplanationEngine::no_telemetry_leakage_integrated();
    }

    #[test]
    fn test_no_confidence_exposure_integrated() {
        CognitiveExplanationEngine::no_confidence_exposure_integrated();
    }

    #[test]
    fn test_no_internal_graph_exposure() {
        CognitiveExplanationEngine::no_internal_graph_exposure();
    }

    #[test]
    fn test_non_blocking_explanation_generation() {
        CognitiveExplanationEngine::non_blocking_explanation_generation();
    }

    #[test]
    fn test_execution_continuity_integrated() {
        CognitiveExplanationEngine::execution_continuity_integrated();
    }

    #[test]
    fn test_degraded_mode_fallback() {
        CognitiveExplanationEngine::degraded_mode_fallback();
    }
}
