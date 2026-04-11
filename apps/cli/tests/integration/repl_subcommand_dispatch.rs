use design_cli::nl::session::ConversationState;
use design_cli::planner::PlannerMode;
use design_cli::repl::dispatch_repl_input;
use design_cli::session::AgentSession;

fn dispatch(input: &str) -> String {
    let mut session = AgentSession::new();
    let mut conversation = ConversationState::default();
    let mut planner_mode = PlannerMode::default();
    let mut writer = Vec::new();
    let should_exit = dispatch_repl_input(
        input,
        &mut session,
        &mut conversation,
        &mut planner_mode,
        &mut writer,
    )
    .expect("dispatch");
    assert!(!should_exit);
    String::from_utf8_lossy(&writer).to_string()
}

#[test]
fn slash_structure_view_bypasses_planner() {
    let output = dispatch("/structure view .");
    assert!(output.contains("[direct-dispatch] structure view ."));
    assert!(!output.contains("[planner:"));
}

#[test]
fn slash_simulate_is_enabled() {
    let output = dispatch("/simulate --steps 4");
    assert!(output.contains("[direct-dispatch] simulate --steps 4"));
}

#[test]
fn slash_rules_and_memory_are_enabled() {
    let rules = dispatch("/rules list");
    assert!(rules.contains("[direct-dispatch] rules list"));

    let memory = dispatch("/memory import seeds/knowledge.json");
    assert!(memory.contains("[direct-dispatch] memory import seeds/knowledge.json"));
}

#[test]
fn natural_language_analyze_routes_to_analyze_plan() {
    let output = dispatch("このプロジェクトを解析して");
    assert!(output.contains("[planner:"));
    assert!(output.contains("[test] planner-only mode"));
}
