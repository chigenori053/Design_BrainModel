use crate::nl::multilingual_router::route_multilingual_intent;
use crate::nl::session::ConversationState;
use crate::nl::types::{IntentType, SupportedLanguage};

#[derive(Clone, Debug, PartialEq)]
pub struct PlannerIntent {
    pub primary_intent: IntentType,
    pub secondary_intents: Vec<IntentType>,
    pub ambiguity_score: f32,
    pub quoted_terms: Vec<String>,
    pub imperative: bool,
    pub confidence: f32,
    pub detected_language: SupportedLanguage,
    pub mixed_language: bool,
}

pub fn infer_planner_intent(input: &str, state: &ConversationState) -> PlannerIntent {
    let routed = route_multilingual_intent(input, state);

    PlannerIntent {
        primary_intent: routed.primary_intent,
        secondary_intents: routed.secondary_intents,
        ambiguity_score: routed.ambiguity_score,
        quoted_terms: routed.quoted_terms,
        imperative: routed.imperative,
        confidence: routed.confidence,
        detected_language: routed.detected_language,
        mixed_language: routed.mixed_language,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::session::ConversationState;

    #[test]
    fn preserves_quoted_order() {
        assert_eq!(
            crate::nl::multilingual_router::extract_quoted_terms(
                "「学習」「失敗から」「ルール生成」"
            ),
            vec![
                "学習".to_string(),
                "失敗から".to_string(),
                "ルール生成".to_string()
            ]
        );
    }

    #[test]
    fn infers_rules_learn_from_quoted_phrases() {
        let intent = infer_planner_intent(
            "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して",
            &ConversationState::default(),
        );
        assert_eq!(intent.primary_intent, IntentType::RulesLearn);
        assert!(
            intent
                .secondary_intents
                .contains(&IntentType::MetaPlannerEdit)
        );
    }

    #[test]
    fn keeps_rules_list_exact_command_stable() {
        let intent = infer_planner_intent("rules list", &ConversationState::default());
        assert_eq!(intent.primary_intent, IntentType::RulesList);
        assert_eq!(intent.detected_language, SupportedLanguage::English);
    }

    #[test]
    fn detects_mixed_language_semantic_bridge() {
        let intent = infer_planner_intent(
            "`rules learn` を planner routing で優先するよう fix して",
            &ConversationState::default(),
        );
        assert!(intent.mixed_language);
        assert_eq!(intent.detected_language, SupportedLanguage::English);
    }
}
