use std::path::PathBuf;

use design_cli::nl::language_intent_bridge::infer_planner_intent;
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::{IntentType, PlannedStep};
use design_cli::session::AgentSession;

fn assert_deterministic_plan(input: &str, conversation: &ConversationState) {
    let session = AgentSession::new();
    let plans = std::iter::repeat_with(|| plan_input(input, &session, conversation).expect("plan"))
        .take(2)
        .collect::<Vec<_>>();
    assert!(plans.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn quoted_semantic_learn_prefers_rules_learn() {
    let intent = infer_planner_intent(
        "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して",
        &ConversationState::default(),
    );
    assert_eq!(intent.primary_intent, IntentType::RulesLearn);
}

#[test]
fn meta_planner_edit_synthesizes_coding_step() {
    let session = AgentSession::new();
    let plan = plan_input(
        "「学習」「失敗から」「ルール生成」で rules learn を優先するよう修正して",
        &session,
        &ConversationState::default(),
    )
    .expect("plan");
    assert!(
        plan.steps
            .iter()
            .any(|step| matches!(step, PlannedStep::Coding(_, _)))
    );
}

#[test]
fn deterministic_replay_matches_exactly() {
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        ..ConversationState::default()
    };
    assert_deterministic_plan(
        "さっきの unresolved import 失敗から学習して次回は自動修正して",
        &conversation,
    );
}

#[test]
fn rules_list_has_no_lexical_regression() {
    let session = AgentSession::new();
    let plan = plan_input("rules list", &session, &ConversationState::default()).expect("plan");
    assert_eq!(plan.steps, vec![PlannedStep::Rules]);
}
