use language_core::{LanguageState, semantic_parser};
use language_reasoning::meaning_reasoning_search;

use crate::nl::intent_ranker::{SemanticFeatures, normalize, rank_intents};
use crate::nl::language_detection::{LanguageDetectionResult, detect_language};
use crate::nl::session::ConversationState;
use crate::nl::types::{IntentType, SupportedLanguage};

#[derive(Clone, Debug, PartialEq)]
pub struct MultilingualRoutingResult {
    pub primary_intent: IntentType,
    pub secondary_intents: Vec<IntentType>,
    pub ambiguity_score: f32,
    pub quoted_terms: Vec<String>,
    pub imperative: bool,
    pub confidence: f32,
    pub detected_language: SupportedLanguage,
    pub mixed_language: bool,
}

pub fn route_multilingual_intent(
    input: &str,
    state: &ConversationState,
) -> MultilingualRoutingResult {
    let quoted_terms = extract_quoted_terms(input);
    let imperative = detect_imperative(input);
    let normalized_input = normalize(input);
    let normalized_tokens = normalized_input
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let language_detection = detect_language(input);

    let references_previous_context = state.last_target.is_some()
        && [
            "さっき",
            "先ほど",
            "前回",
            "次回",
            "前の",
            "last",
            "previous",
            "earlier",
        ]
        .iter()
        .any(|needle| input.to_lowercase().contains(needle));

    let semantic_terms = quoted_terms
        .iter()
        .map(|term| normalize(term))
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();

    let features = SemanticFeatures {
        normalized_input,
        normalized_tokens,
        semantic_terms,
        quoted_terms: quoted_terms
            .iter()
            .map(|term| normalize(term))
            .filter(|term| !term.is_empty())
            .collect(),
        imperative,
        references_previous_context,
        detected_language: language_detection.detected_language,
        mixed_language: language_detection.mixed_language,
    };

    let parsed = bridge_language_core(input);
    let ranked = rank_intents(&features, &parsed);
    let top = ranked.first().cloned();
    let runner_up = ranked.get(1).cloned();
    let primary_intent = top
        .as_ref()
        .map(|candidate| candidate.intent)
        .unwrap_or(IntentType::Unknown);
    let top_score = top.as_ref().map(|candidate| candidate.score).unwrap_or(0.0);
    let top2_score = runner_up
        .as_ref()
        .map(|candidate| candidate.score)
        .unwrap_or(0.0);
    let ambiguity_score = language_aware_ambiguity_score(
        top_score,
        top2_score,
        &quoted_terms,
        imperative,
        language_detection,
    );
    let confidence = top_score.clamp(0.0, 1.0);
    let secondary_intents = ranked
        .into_iter()
        .skip(1)
        .filter(|candidate| candidate.score >= 0.30)
        .map(|candidate| candidate.intent)
        .collect::<Vec<_>>();

    MultilingualRoutingResult {
        primary_intent,
        secondary_intents,
        ambiguity_score,
        quoted_terms,
        imperative,
        confidence,
        detected_language: language_detection.detected_language,
        mixed_language: language_detection.mixed_language,
    }
}

pub fn bridge_language_core(input: &str) -> LanguageState {
    let mut parsed = semantic_parser(input);
    parsed.semantic_graph = meaning_reasoning_search(parsed.semantic_graph.clone());
    parsed
}

pub fn detect_imperative(input: &str) -> bool {
    let lower = input.to_lowercase();
    [
        "して",
        "してくれ",
        "してください",
        "しろ",
        "してほしい",
        "直して",
        "修正して",
        "変更して",
        "fix",
        "edit",
        "update",
        "patch",
        "please",
        "show me",
        "inspect",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub fn extract_quoted_terms(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut active_quote = None;
    let mut start = 0;

    for (index, ch) in input.char_indices() {
        match (active_quote, ch) {
            (None, '「' | '『' | '"' | '\'' | '“' | '‘' | '`') => {
                active_quote = Some(ch);
                start = index + ch.len_utf8();
            }
            (Some('「'), '」')
            | (Some('『'), '』')
            | (Some('"'), '"')
            | (Some('\''), '\'')
            | (Some('“'), '”')
            | (Some('‘'), '’')
            | (Some('`'), '`') => {
                if start <= index {
                    let segment = input[start..index].trim();
                    if !segment.is_empty() {
                        terms.push(segment.to_string());
                    }
                }
                active_quote = None;
            }
            _ => {}
        }
    }

    terms
}

fn language_aware_ambiguity_score(
    top_score: f32,
    top2_score: f32,
    quoted_terms: &[String],
    imperative: bool,
    detection: LanguageDetectionResult,
) -> f32 {
    let mut ambiguity = (1.0 - (top_score - top2_score)).clamp(0.0, 1.0);

    if detection.mixed_language {
        ambiguity += 0.08;
    }

    if detection.detected_language == SupportedLanguage::Unknown {
        ambiguity += 0.12;
    }

    if !quoted_terms.is_empty() {
        ambiguity -= 0.06;
    }

    if imperative {
        ambiguity -= 0.04;
    }

    if top_score >= 0.75 {
        ambiguity -= 0.15;
    }

    ambiguity.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bilingual_quoted_terms() {
        assert_eq!(
            extract_quoted_terms("`rules learn` と 「失敗から学習」 を bridge して"),
            vec!["rules learn".to_string(), "失敗から学習".to_string()]
        );
    }

    #[test]
    fn routes_mixed_language_request_deterministically() {
        let state = ConversationState::default();
        let baseline =
            route_multilingual_intent("planner routing を fix して validate して", &state);
        let replay = route_multilingual_intent("planner routing を fix して validate して", &state);
        assert_eq!(baseline, replay);
        assert!(baseline.mixed_language);
    }

    #[test]
    fn ambiguity_decreases_for_clear_imperative_quotes() {
        let clear = language_aware_ambiguity_score(
            0.82,
            0.35,
            &["rules learn".to_string()],
            true,
            detect_language("`rules learn` を修正して"),
        );
        let vague = language_aware_ambiguity_score(
            0.55,
            0.46,
            &[],
            false,
            detect_language("planner maybe"),
        );
        assert!(clear < vague);
    }
}
