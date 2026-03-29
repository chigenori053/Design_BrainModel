use crate::commands::analyze::project::UnifiedAnalyzeResult;
use crate::renderer::formatter::{confidence_label, format_score};

pub fn render_decision(result: &UnifiedAnalyzeResult) -> String {
    format!(
        "Decision Context\nTop Recommendation:\n- Action: {}\n- Expected Impact: {}\n- Confidence: {} ({})\n- Risk: {}\n- Intent Match: {}",
        result.decision.action,
        result.decision.expected_impact,
        confidence_label(result.decision.confidence),
        format_score(result.decision.confidence),
        result.decision.risk,
        result.decision.intent_match,
    )
}
