use std::path::PathBuf;

use design_cli::nl::intent::primary_intent;
use design_cli::nl::planner::plan_input;
use design_cli::nl::target::resolve_target;
use design_cli::nl::types::{CodingOptions, IntentType, PlannedStep};
use design_cli::session::AgentSession;

#[test]
fn nl_intent_routes_analysis_and_structure_and_coding() {
    assert_eq!(primary_intent("このプロジェクトを解析して"), IntentType::Analyze);
    assert_eq!(primary_intent("GUIで構造を開いて"), IntentType::StructureView);
    assert_eq!(primary_intent("unsafeを減らして cargo check"), IntentType::Coding);
}

#[test]
fn nl_target_resolution_uses_project_phrase_and_last_path() {
    let session = AgentSession::new();
    let project = resolve_target("このプロジェクト全体を解析して", &session);
    assert_eq!(project.path, PathBuf::from("."));

    let mut session = AgentSession::new();
    session.context.set_last_path("apps/cli");
    let fallback = resolve_target("安全に修正して", &session);
    assert_eq!(fallback.path, PathBuf::from("apps/cli"));
}

#[test]
fn nl_multi_step_plan_enforces_safe_coding_defaults() {
    let session = AgentSession::new();
    let plan = plan_input("unsafeを減らして cargo check して", &session).expect("plan");
    assert_eq!(
        plan.steps,
        vec![
            PlannedStep::Analyze(PathBuf::from(".")),
            PlannedStep::Coding(PathBuf::from("."), CodingOptions::default()),
            PlannedStep::Validate(PathBuf::from(".")),
        ]
    );
}
