use language_core::LanguageState;

use crate::nl::types::{IntentType, SupportedLanguage};

const INTENT_ORDER: [IntentType; 7] = [
    IntentType::RulesLearn,
    IntentType::RulesList,
    IntentType::CodingEdit,
    IntentType::AnalyzeArchitecture,
    IntentType::Validate,
    IntentType::StructureView,
    IntentType::MetaPlannerEdit,
];

#[derive(Clone, Debug, PartialEq)]
pub struct RankedIntent {
    pub intent: IntentType,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticFeatures {
    pub normalized_input: String,
    pub normalized_tokens: Vec<String>,
    pub semantic_terms: Vec<String>,
    pub quoted_terms: Vec<String>,
    pub imperative: bool,
    pub references_previous_context: bool,
    pub detected_language: SupportedLanguage,
    pub mixed_language: bool,
}

pub fn rank_intents(features: &SemanticFeatures, parsed: &LanguageState) -> Vec<RankedIntent> {
    let semantic_terms = parsed
        .semantic_graph
        .concepts
        .values()
        .map(|concept| normalize(&concept.label))
        .collect::<Vec<_>>();

    let mut enriched = features.clone();
    enriched.semantic_terms.extend(semantic_terms);
    enriched.semantic_terms.sort();
    enriched.semantic_terms.dedup();

    let mut ranked = INTENT_ORDER
        .iter()
        .copied()
        .map(|intent| RankedIntent {
            intent,
            score: score_intent(intent, &enriched),
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| intent_order(left.intent).cmp(&intent_order(right.intent)))
    });
    ranked
}

fn score_intent(intent: IntentType, features: &SemanticFeatures) -> f32 {
    let base = match intent {
        IntentType::RulesLearn => weighted_similarity(
            features,
            &[
                (
                    0.28,
                    &[
                        "learn",
                        "learning",
                        "学習",
                        "失敗から学習",
                        "failure-driven",
                    ],
                ),
                (0.22, &["失敗", "failure", "failed", "unresolved import"]),
                (
                    0.18,
                    &[
                        "ルール生成",
                        "rule generation",
                        "rules learn",
                        "rules learn を優先",
                    ],
                ),
                (0.14, &["自動修正", "autofix", "次回は自動修正"]),
            ],
        ),
        IntentType::RulesList => weighted_similarity(
            features,
            &[
                (0.42, &["rules list", "rule list", "list rules"]),
                (0.16, &["rules", "rule"]),
                (0.16, &["ルール一覧", "ルールを表示", "一覧"]),
            ],
        ),
        IntentType::CodingEdit => weighted_similarity(
            features,
            &[
                (0.24, &["修正", "fix", "edit", "変更", "直して", "refactor"]),
                (0.18, &["priority", "優先", "優先するように", "ようにする"]),
                (0.12, &["自動修正", "autofix"]),
            ],
        ),
        IntentType::AnalyzeArchitecture => weighted_similarity(
            features,
            &[
                (0.22, &["analyze", "解析", "分析", "調べて"]),
                (0.20, &["planner", "routing", "precedence", "intent"]),
                (0.12, &["architecture", "設計", "構造"]),
            ],
        ),
        IntentType::Validate => weighted_similarity(
            features,
            &[
                (0.36, &["validate", "検証", "lint", "cargo check", "check"]),
                (0.12, &["確認"]),
            ],
        ),
        IntentType::StructureView => weighted_similarity(
            features,
            &[
                (0.28, &["gui", "viewer", "graph", "構造"]),
                (0.18, &["見せて", "開いて", "view"]),
            ],
        ),
        IntentType::MetaPlannerEdit => weighted_similarity(
            features,
            &[
                (0.18, &["planner", "routing", "precedence", "intent"]),
                (
                    0.26,
                    &["修正", "変更", "優先", "ようにする", "edit", "fix", "patch"],
                ),
                (0.12, &["learn", "学習", "rules learn"]),
            ],
        ),
        _ => 0.0,
    };

    let mut score = base;

    if features.imperative && matches!(intent, IntentType::CodingEdit | IntentType::MetaPlannerEdit)
    {
        score += 0.12;
    }

    if features.references_previous_context
        && matches!(intent, IntentType::RulesLearn | IntentType::MetaPlannerEdit)
    {
        score += 0.08;
    }

    if features.detected_language == SupportedLanguage::Japanese
        && matches!(
            intent,
            IntentType::RulesLearn
                | IntentType::CodingEdit
                | IntentType::AnalyzeArchitecture
                | IntentType::Validate
        )
    {
        score += 0.04;
    }

    if features.mixed_language
        && matches!(
            intent,
            IntentType::RulesLearn | IntentType::CodingEdit | IntentType::MetaPlannerEdit
        )
    {
        score += 0.05;
    }

    if quoted_learn_signal(features) && intent == IntentType::RulesLearn {
        score += 0.32;
    }

    if meta_edit_signal(features) && intent == IntentType::MetaPlannerEdit {
        score += 0.34;
    }

    if !meta_edit_signal(features) && intent == IntentType::MetaPlannerEdit {
        score -= 0.18;
    }

    if exact_rules_list_signal(features) {
        if intent == IntentType::RulesList {
            score += 0.45;
        } else if matches!(intent, IntentType::RulesLearn | IntentType::MetaPlannerEdit) {
            score -= 0.18;
        }
    }

    if validation_signal(features) {
        if intent == IntentType::Validate {
            score += 0.22;
        } else if intent == IntentType::MetaPlannerEdit {
            score -= 0.12;
        }
    }

    if analyze_signal(features) && intent == IntentType::AnalyzeArchitecture {
        score += 0.12;
    }

    score.clamp(0.0, 1.0)
}

fn weighted_similarity(features: &SemanticFeatures, groups: &[(f32, &[&str])]) -> f32 {
    groups
        .iter()
        .map(|(weight, cues)| {
            *weight
                * cues
                    .iter()
                    .map(|cue| semantic_similarity(cue, features))
                    .fold(0.0_f32, f32::max)
        })
        .sum::<f32>()
}

fn semantic_similarity(cue: &str, features: &SemanticFeatures) -> f32 {
    let cue = normalize(cue);
    if cue.is_empty() {
        return 0.0;
    }

    let mut best = compare_feature(&cue, &features.normalized_input);
    for feature in features
        .normalized_tokens
        .iter()
        .chain(features.semantic_terms.iter())
        .chain(features.quoted_terms.iter())
    {
        best = best.max(compare_feature(&cue, feature));
    }
    best
}

fn compare_feature(cue: &str, feature: &str) -> f32 {
    if cue == feature {
        return 1.0;
    }
    if feature.contains(cue) || cue.contains(feature) {
        return 0.88;
    }

    let cue_bigrams = bigrams(cue);
    let feature_bigrams = bigrams(feature);
    if cue_bigrams.is_empty() || feature_bigrams.is_empty() {
        return 0.0;
    }
    let overlap = cue_bigrams
        .iter()
        .filter(|gram| feature_bigrams.contains(*gram))
        .count();
    let denom = cue_bigrams.len() + feature_bigrams.len();
    if denom == 0 {
        0.0
    } else {
        (2.0 * overlap as f32 / denom as f32).min(0.84)
    }
}

fn bigrams(text: &str) -> Vec<String> {
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect::<Vec<_>>()
}

fn quoted_learn_signal(features: &SemanticFeatures) -> bool {
    if features.quoted_terms.len() < 2 {
        return false;
    }
    let joined = features.quoted_terms.join(" ");
    ["失敗", "学習", "ルール生成"]
        .iter()
        .all(|needle| joined.contains(needle))
}

fn meta_edit_signal(features: &SemanticFeatures) -> bool {
    let input = &features.normalized_input;
    let planner_like = ["planner", "routing", "precedence", "learn"]
        .iter()
        .any(|needle| input.contains(needle));
    let edit_like = ["修正", "変更", "優先", "ようにする", "fix", "edit"]
        .iter()
        .any(|needle| input.contains(needle));
    planner_like && edit_like
}

fn exact_rules_list_signal(features: &SemanticFeatures) -> bool {
    matches!(
        features.normalized_input.as_str(),
        "rules list" | "rule list"
    )
}

fn validation_signal(features: &SemanticFeatures) -> bool {
    let input = &features.normalized_input;
    ["validate", "検証", "check", "lint", "確認"]
        .iter()
        .any(|needle| input.contains(needle))
}

fn analyze_signal(features: &SemanticFeatures) -> bool {
    let input = &features.normalized_input;
    ["analyze", "analyse", "解析", "分析", "調べ"]
        .iter()
        .any(|needle| input.contains(needle))
}

fn intent_order(intent: IntentType) -> usize {
    INTENT_ORDER
        .iter()
        .position(|candidate| *candidate == intent)
        .unwrap_or(INTENT_ORDER.len())
}

pub fn normalize(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|ch| match ch {
            '\u{3000}' | '\n' | '\t' | '\r' | ',' | '。' | '、' | ';' | ':' | '(' | ')' => ' ',
            '「' | '」' | '『' | '』' | '"' | '\'' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use language_core::semantic_parser;

    use super::*;

    fn features(input: &str) -> SemanticFeatures {
        SemanticFeatures {
            normalized_input: normalize(input),
            normalized_tokens: normalize(input)
                .split_whitespace()
                .map(ToString::to_string)
                .collect(),
            semantic_terms: Vec::new(),
            quoted_terms: vec![
                "学習".to_string(),
                "失敗から".to_string(),
                "ルール生成".to_string(),
            ],
            imperative: true,
            references_previous_context: false,
            detected_language: SupportedLanguage::Japanese,
            mixed_language: true,
        }
    }

    #[test]
    fn quoted_learn_signal_beats_rules_list() {
        let parsed = semantic_parser(
            "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して",
        );
        let ranked = rank_intents(
            &features("「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して"),
            &parsed,
        );
        assert_eq!(ranked[0].intent, IntentType::RulesLearn);
    }
}
