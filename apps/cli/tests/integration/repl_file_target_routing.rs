use std::path::PathBuf;

use design_cli::nl::intent::{wants_analyze, wants_coding};
use design_cli::nl::planner_v2::plan_input;
use design_cli::nl::session::ConversationState;
use design_cli::nl::types::Operation;
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

// ─── Case 1: file-local coding → Refactor { target } ─────────────────────────

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
        assert_eq!(
            plan.operation,
            Operation::Refactor,
            "must produce Refactor for: {input}"
        );
        assert_eq!(
            plan.target,
            Some(PathBuf::from(expected_path)),
            "must route to file target for: {input}"
        );
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

        assert_eq!(
            plan.operation,
            Operation::Refactor,
            "must be Refactor for: {input}"
        );
        assert_eq!(
            plan.target,
            Some(PathBuf::from(prev)),
            "must reuse last target for: {input}"
        );
    }
}

#[test]
fn analyze_intent_does_not_route_to_coding() {
    let project_wide = plan_input("プロジェクト全体を解析して", &session(), &conv());
    if let Some(plan) = project_wide {
        assert_ne!(
            plan.operation,
            Operation::Refactor,
            "project-wide analyze must not produce a Refactor operation"
        );
    }

    let analyze =
        plan_input("このプロジェクトを解析して", &session(), &conv()).expect("must produce a plan");
    assert_ne!(
        analyze.operation,
        Operation::Refactor,
        "解析 without mutation must not produce Refactor"
    );
    assert_eq!(
        analyze.operation,
        Operation::Analyze,
        "解析 must produce Analyze"
    );
}

#[test]
fn non_mutating_file_reference_does_not_force_coding() {
    let plan = plan_input("apps/cli/src/coding.rs を見せて", &session(), &conv());

    if let Some(plan) = plan {
        assert_ne!(
            plan.operation,
            Operation::Refactor,
            "view intent must not produce Refactor"
        );
    }

    assert!(wants_analyze("このプロジェクトを解析して"));
    assert!(!wants_analyze("coding.rs を厳密化して"));
}

// ─── R5 executor remapping ────────────────────────────────────────────────────

#[test]
fn coding_step_with_rs_path_remaps_to_target_flag() {
    use design_cli::nl::render_plan_summary_with_label;
    use design_cli::nl::types::{ExecutionPlan, Operation, PlanSource};

    let plan = ExecutionPlan::new(
        Operation::Refactor,
        Some(PathBuf::from("apps/cli/src/coding.rs")),
        PlanSource::ReplInput,
    );

    let summary = render_plan_summary_with_label(&plan, "test");
    assert!(
        summary.contains("Refactor") || summary.contains("refactor"),
        "plan summary must contain operation name: {summary}"
    );
    assert!(
        summary.contains("coding.rs"),
        "plan summary must contain target path: {summary}"
    );
}
