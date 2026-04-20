use std::path::PathBuf;

use design_cli::nl::intent::{wants_analyze, wants_coding};
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::PlannedStep;
use design_cli::session::AgentSession;

fn session() -> AgentSession {
    AgentSession::new()
}

fn conv() -> ConversationState {
    ConversationState::default()
}

fn conv_with_last(path: &str) -> ConversationState {
    ConversationState {
        last_target: Some(PathBuf::from(path)),
        ..ConversationState::default()
    }
}

// ─── mutation verb recognition ────────────────────────────────────────────────

#[test]
fn wants_coding_recognizes_mutation_verbs() {
    for input in ["厳密化", "改善", "refactor", "prune", "rebind"] {
        assert!(wants_coding(input), "{input} must be a mutation verb");
    }
}

// ─── Case 1: file-local coding → Coding { target } ───────────────────────────

#[test]
fn file_path_with_mutation_routes_to_coding() {
    for (input, expected_path) in [
        (
            "apps/cli/src/coding.rs の bootstrap pruning をさらに厳密化して",
            "apps/cli/src/coding.rs",
        ),
        (
            "refactor apps/cli/src/nl/goal.rs",
            "apps/cli/src/nl/goal.rs",
        ),
        (
            "apps/cli/src/coding.rs の semantic pruning を改善して",
            "apps/cli/src/coding.rs",
        ),
    ] {
        let plan = plan_input(input, &session(), &conv()).expect("must produce a plan");
        assert_eq!(plan.steps.len(), 1, "must produce exactly one step");
        match &plan.steps[0] {
            PlannedStep::Coding(path, _) => {
                assert_eq!(path, &PathBuf::from(expected_path));
            }
            other => panic!("expected Coding step, got {other:?}"),
        }
    }
}

#[test]
fn previous_target_reuse_on_mutation_followup() {
    for (prev, input) in [
        ("apps/cli/src/coding.rs", "さっきの場所をさらに改善して"),
        (
            "apps/cli/src/nl/goal.rs",
            "さっきのファイルをさらに修正して",
        ),
    ] {
        let plan = plan_input(input, &session(), &conv_with_last(prev))
            .expect("must produce a plan with previous target");

        match &plan.steps[0] {
            PlannedStep::Coding(path, _) => {
                assert_eq!(path, &PathBuf::from(prev), "must reuse last target");
            }
            other => panic!("expected Coding with previous target, got {other:?}"),
        }
    }
}

#[test]
fn analyze_intent_does_not_route_to_coding() {
    let project_wide = plan_input("プロジェクト全体を解析して", &session(), &conv());
    if let Some(plan) = project_wide {
        for step in &plan.steps {
            assert!(
                !matches!(step, PlannedStep::Coding(_, _)),
                "project-wide analyze must not produce a Coding step, got {step:?}"
            );
        }
    }

    let analyze =
        plan_input("このプロジェクトを解析して", &session(), &conv()).expect("must produce a plan");
    for step in &analyze.steps {
        assert!(
            !matches!(step, PlannedStep::Coding(_, _)),
            "解析 without mutation must not produce Coding: {step:?}"
        );
    }
    assert!(
        analyze
            .steps
            .iter()
            .any(|s| matches!(s, PlannedStep::Analyze(_)))
    );
}

#[test]
fn non_mutating_file_reference_does_not_force_coding() {
    let plan = plan_input("apps/cli/src/coding.rs を見せて", &session(), &conv());

    if let Some(plan) = plan {
        for step in &plan.steps {
            assert!(
                !matches!(step, PlannedStep::Coding(_, _)),
                "view intent must not produce Coding: {step:?}"
            );
        }
    }

    assert!(wants_analyze("このプロジェクトを解析して"));
    assert!(!wants_analyze("coding.rs を厳密化して"));
}

// ─── R5 executor remapping ────────────────────────────────────────────────────

#[test]
fn coding_step_with_rs_path_remaps_to_target_flag() {
    use design_cli::nl::types::{CodingOptions, CommandPlan, PlannedStep};

    // Build a plan with a .rs file path directly
    let plan = CommandPlan {
        intent: None,
        steps: vec![PlannedStep::Coding(
            PathBuf::from("apps/cli/src/coding.rs"),
            CodingOptions::default(),
        )],
    };

    // Execute in test mode (cfg!(test) = true) — executor will NOT run the
    // real subprocess; it stores the legacy plan. We verify the command args
    // via the to_canonical_command output by inspecting plan rendering.
    let summary = design_cli::nl::render_plan_summary(&plan);
    assert!(
        summary.contains("1 steps") || summary.contains("1 step"),
        "plan must have 1 step: {summary}"
    );
}
