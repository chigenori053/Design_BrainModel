use std::path::PathBuf;

use design_cli::nl::context::merge_target;
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::Operation;
use design_cli::session::AgentSession;

#[test]
fn context_merge_inherits_target_and_node() {
    let session = AgentSession::new();
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        last_node: Some("presentation".to_string()),
        ..ConversationState::default()
    };
    let merged = merge_target("presentation layer 側だけ直して", &session, &conversation);
    assert_eq!(merged.path, PathBuf::from("."));
    assert_eq!(merged.node.as_deref(), Some("presentation"));
}

#[test]
fn mutation_followup_routes_to_refactor() {
    let session = AgentSession::new();
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        ..ConversationState::default()
    };
    let plan =
        plan_input("presentation layer 側だけ直して", &session, &conversation).expect("plan");
    assert_eq!(plan.operation, Operation::Refactor);
}

#[test]
fn analyze_only_does_not_produce_refactor() {
    let session = AgentSession::new();
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        ..ConversationState::default()
    };
    let plan = plan_input("プロジェクト全体を解析して", &session, &conversation);
    if let Some(plan) = plan {
        assert_ne!(plan.operation, Operation::Refactor);
    }
}
