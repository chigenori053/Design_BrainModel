use std::path::PathBuf;

use design_cli::nl::language_intent_bridge::infer_planner_intent;
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::{IntentType, Operation, SupportedLanguage};
use design_cli::session::AgentSession;

fn assert_replay_is_deterministic(input: &str, conversation: &ConversationState) {
    let session = AgentSession::new();
    let plans = std::iter::repeat_with(|| plan_input(input, &session, conversation).expect("plan"))
        .take(3)
        .collect::<Vec<_>>();
    assert!(plans.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn english_semantic_request_detects_language_and_routes_validate() {
    let intent = infer_planner_intent(
        "please validate planner routing",
        &ConversationState::default(),
    );
    assert_eq!(intent.detected_language, SupportedLanguage::English);
    assert!(!intent.mixed_language);
    assert_eq!(intent.primary_intent, IntentType::Validate);
}

#[test]
fn japanese_semantic_request_prefers_semantic_frontend() {
    let session = AgentSession::new();
    let plan = plan_input(
        "planner の routing を解析して検証して",
        &session,
        &ConversationState::default(),
    )
    .expect("plan");
    // Validate is not a top-level Operation; the primary operation is Analyze
    assert_eq!(plan.operation, Operation::Analyze);
}

#[test]
fn mixed_language_request_preserves_bilingual_quotes() {
    let intent = infer_planner_intent(
        "`rules learn` を「失敗から学習」と一緒に planner routing へ bridge して fix して",
        &ConversationState::default(),
    );
    assert!(intent.mixed_language);
    assert_eq!(intent.quoted_terms, vec!["rules learn", "失敗から学習"]);
    assert!(intent.ambiguity_score < 1.0);
}

#[test]
fn mixed_language_continuation_replay_is_deterministic() {
    let conversation = ConversationState {
        last_target: Some(PathBuf::from("apps/cli/src/nl/planner_v2.rs")),
        ..ConversationState::default()
    };
    assert_replay_is_deterministic(
        "さっきの planner routing を fix して validate して",
        &conversation,
    );
}
