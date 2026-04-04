use std::path::PathBuf;

use design_cli::nl::context::merge_target;
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::{CodingOptions, PlannedStep};
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
fn viewer_undo_maps_from_conversation_state() {
    let session = AgentSession::new();
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        last_viewer_session: Some("viewer-session".to_string()),
        ..ConversationState::default()
    };
    let plan = plan_input("1つ戻して", &session, &conversation).expect("plan");
    assert_eq!(
        plan.steps,
        vec![PlannedStep::StructureUndo(PathBuf::from("."))]
    );
}

#[test]
fn git_steps_default_to_dry_run_safe_workflow() {
    let session = AgentSession::new();
    let conversation = ConversationState {
        last_target: Some(PathBuf::from(".")),
        ..ConversationState::default()
    };
    let plan = plan_input("commitしてPR作って", &session, &conversation).expect("plan");
    assert_eq!(
        plan.steps,
        vec![
            PlannedStep::GitCommit(PathBuf::from(".")),
            PlannedStep::GitPR(PathBuf::from("."))
        ]
    );
    let followup = plan_input("presentation layer 側だけ直して", &session, &conversation)
        .expect("followup plan");
    assert_eq!(
        followup.steps,
        vec![PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions::default()
        )]
    );
}
