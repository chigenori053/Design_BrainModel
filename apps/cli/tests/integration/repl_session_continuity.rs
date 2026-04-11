use std::path::PathBuf;

use design_cli::nl::session::ConversationState;
use design_cli::planner::PlannerMode;
use design_cli::repl::{dispatch_repl_input, reset_review_session};
use design_cli::session::AgentSession;
use design_cli::state::State;
use design_cli::tui::composer::{ComposerFocus, ComposerUiMode, ComposerViewState};
use design_cli::tui::review_batch::ReviewBatchState;

fn reset_and_dispatch(next_input: &str) -> (ComposerViewState, AgentSession, String) {
    let mut session = AgentSession::new();
    session.state = State::Completed;
    let mut view = ComposerViewState::new(Vec::new(), session.state);
    view.activate_review(ReviewBatchState::empty(PathBuf::from(".")));
    view.focus = ComposerFocus::SendButton;

    reset_review_session(&mut view, &mut session);

    let submit = {
        view.buffer.insert_str(next_input);
        view.handle_key_event(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ))
    };
    assert!(
        matches!(submit, design_cli::tui::composer::ComposerAction::Submit(_)),
        "next turn must be accepted after reset"
    );

    let mut writer = Vec::new();
    let mut conversation = ConversationState::default();
    let mut planner_mode = PlannerMode::default();
    let should_exit = dispatch_repl_input(
        next_input,
        &mut session,
        &mut conversation,
        &mut planner_mode,
        &mut writer,
    )
    .expect("dispatch");
    assert!(!should_exit);
    (view, session, String::from_utf8_lossy(&writer).to_string())
}

#[test]
fn apply_then_next_analyze_restarts_planner() {
    let (view, session, output) = reset_and_dispatch("このプロジェクトを解析して");
    assert_eq!(view.mode, ComposerUiMode::Idle);
    assert_eq!(view.focus, ComposerFocus::Editor);
    assert!(view.review.is_none());
    assert_eq!(session.state, State::Completed);
    assert!(output.contains("[planner:") || output.contains("[test] planner-only mode"));
}

#[test]
fn rollback_then_next_coding_restarts_planner() {
    let (view, session, output) = reset_and_dispatch("apps/cli/src/coding.rs を修正して");
    assert_eq!(view.mode, ComposerUiMode::Idle);
    assert_eq!(view.focus, ComposerFocus::Editor);
    assert!(view.review.is_none());
    assert_eq!(session.state, State::Completed);
    assert!(output.contains("[planner:") || output.contains("[test] planner-only mode"));
}

#[test]
fn discard_then_next_structure_is_accepted() {
    let (view, session, output) = reset_and_dispatch("/structure view .");
    assert_eq!(view.mode, ComposerUiMode::Idle);
    assert_eq!(view.focus, ComposerFocus::Editor);
    assert!(view.review.is_none());
    assert_eq!(session.state, State::Completed);
    assert!(output.contains("[direct-dispatch] structure view ."));
}
