use design_cli::nl::autonomous::{AutonomousLoop, run_goal_loop};
use design_cli::nl::convergence::{ConvergenceMetrics, goal_reached};
use design_cli::nl::goal::{GoalType, detect_goal};
use design_cli::nl::session::ConversationState;
use design_cli::session::AgentSession;

#[test]
fn goal_detection_works() {
    assert_eq!(detect_goal("この循環依存をゼロにして"), Some(GoalType::EliminateCycles));
    assert_eq!(detect_goal("unsafe を減らして"), Some(GoalType::ReduceUnsafe));
}

#[test]
fn convergence_stop_works() {
    assert!(goal_reached(
        GoalType::EliminateCycles,
        ConvergenceMetrics {
            before: 1.0,
            after: 0.0,
            confidence: 1.0,
            validation_ok: true,
        },
        0.95
    ));
}

#[test]
fn max_iteration_stop_works() {
    let mut session = AgentSession::new();
    let mut conversation = ConversationState::default();
    let result = run_goal_loop(
        GoalType::ReduceUnsafe,
        &mut session,
        &mut conversation,
        AutonomousLoop {
            max_iterations: 1,
            convergence_threshold: 0.95,
        },
    );
    assert!(!result.completed);
    assert!(result.outputs.iter().any(|line| line.contains("max iterations exceeded")));
}

#[test]
fn safe_defaults_and_git_dry_run_are_preserved() {
    let mut session = AgentSession::new();
    let mut conversation = ConversationState::default();
    let result = run_goal_loop(
        GoalType::EliminateCycles,
        &mut session,
        &mut conversation,
        AutonomousLoop::default(),
    );
    assert!(
        result
            .outputs
            .iter()
            .any(|line| line.contains("design_cli coding . --safe --check"))
    );
    assert!(
        result
            .outputs
            .iter()
            .any(|line| line.contains("--dry-run --json [confirmation required, branch != main]"))
    );
}
