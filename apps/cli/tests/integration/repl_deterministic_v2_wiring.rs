use serde_json::Value;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_repl_v2_wiring_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/nl")).expect("create src/nl");
    fs::write(dir.join("src/nl/mod.rs"), "pub mod planner_v2;\n").expect("write nl mod");
    fs::write(
        dir.join("src/nl/planner_v2.rs"),
        "use crate::session::AgentSession;\nuse crate::nl::session::ConversationState;\nuse crate::nl::types::CommandPlan;\npub fn plan_input(_input: &str, _session: &AgentSession, _conversation: &ConversationState) -> Option<CommandPlan> { None }\npub fn update_conversation_after_plan(_input: &str, _plan: &CommandPlan, _conversation: &mut ConversationState) {}\n",
    )
    .expect("write planner_v2");
    fs::write(
        dir.join("src/repl.rs"),
        r#"use std::io::{BufRead, Write};

use crate::command::{CommandRegistry, Output};
use crate::commands::register_defaults;
use crate::executor::Executor;
use crate::input::{InputState, read_input_with_label};
use crate::nl::autonomous::{AutonomousLoop, run_goal_loop};
use crate::nl::goal::{detect_goal, goal_label};
use crate::nl::planner_v2::{plan_input as plan_nl_input_v2, update_conversation_after_plan};
use crate::nl::session::ConversationState;
use crate::nl::{execute_plan as execute_nl_plan, render_plan_summary};
use crate::plan::PlanStatus;
use crate::planner::{PlannerMode, create_plan};
use crate::router::{Route, route};
use crate::session::AgentSession;
use crate::state::State;

fn handle_agent<W: Write>(
    input: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    registry: &CommandRegistry,
    planner_mode: PlannerMode,
    writer: &mut W,
) -> Result<(), String> {
    session.context.push(input);
    session.state = State::Planning;

    if let Some(command_plan) = plan_nl_input_v2(input, session, conversation) {
        let planner_summary = render_plan_summary(&command_plan);
        emit_output(session, writer, &planner_summary)?;
        update_conversation_after_plan(input, &command_plan, conversation);

        if cfg!(test) {
            session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
            session.state = State::Completed;
            emit_output(session, writer, "[test] planner-only mode")?;
            return Ok(());
        }

        session.state = State::Running;
        for output in execute_nl_plan(&command_plan, conversation) {
            if !output.trim().is_empty() {
                emit_output(session, writer, &output)?;
            }
        }
        session.state = State::Completed;
        session.current_plan = Some(crate::nl::to_legacy_plan(&command_plan));
        conversation.autonomous_label = None;
        print_follow_up_suggestions(input, session, writer)?;
        return Ok(());
    }

    let mut plan = create_plan(input, session, planner_mode);
    let planner_summary = format!(
        "[planner: {}] {} ステップ",
        planner_mode.as_str(),
        plan.steps.len(),
    );
    emit_output(session, writer, &planner_summary)?;
    Ok(())
}
"#,
    )
    .expect("write repl");
    dir
}

fn run(dir: &std::path::Path, request: &str) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .current_dir(dir)
        .args([
            "coding",
            ".",
            "--target",
            "src/repl.rs",
            "--check",
            "--no-build",
            "--json",
            "--request",
            request,
        ])
        .output()
        .expect("run design_cli");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

fn replacement(stdout: &str) -> String {
    let out: Value = serde_json::from_str(stdout).expect("stdout json");
    out["changes"]["changes"][0]["hunks"][0]["replacement"]
        .as_str()
        .expect("replacement")
        .to_string()
}

#[test]
fn repl_v2_wiring_injects_import() {
    let dir = temp_workspace("import");
    let (code, stdout, stderr) = run(&dir, "repl.rs の planner_v2 接続を修正して");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        replacement(&stdout).contains("use crate::nl::planner_v2::plan_input as plan_nl_input_v2;"),
        "stdout: {stdout}"
    );
}

#[test]
fn repl_v2_wiring_adds_fallback_chain() {
    let dir = temp_workspace("fallback");
    let (code, stdout, stderr) = run(&dir, "repl.rs の planner_v2 routing を rewrite して");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        replacement(&stdout).contains(".or_else(|| plan_nl_input(input, session))"),
        "stdout: {stdout}"
    );
}

#[test]
fn repl_v2_wiring_is_idempotent_on_second_dry_run() {
    let dir = temp_workspace("noop");
    let request = "repl.rs の planner_v2 接続を修正して";

    let (first_code, first_stdout, first_stderr) = run(&dir, request);
    assert_eq!(first_code, 0, "stderr: {first_stderr}");
    fs::write(dir.join("src/repl.rs"), replacement(&first_stdout)).expect("apply replacement");

    let (second_code, second_stdout, second_stderr) = run(&dir, request);
    assert_eq!(second_code, 0, "stderr: {second_stderr}");

    let out: Value = serde_json::from_str(&second_stdout).expect("stdout json");
    assert_eq!(out["patches"], Value::Array(vec![]), "stdout: {second_stdout}");
    assert_eq!(
        out["changes"]["changes"],
        Value::Array(vec![]),
        "stdout: {second_stdout}"
    );
}

#[test]
fn repl_v2_wiring_emits_no_architectural_frontier_candidates() {
    let dir = temp_workspace("frontier");
    let (code, stdout, stderr) = run(&dir, "repl.rs の planner_v2 接続を修正して");
    assert_eq!(code, 0, "stderr: {stderr}");
    for forbidden in [
        "adapter_app_interface",
        "agent_domain_interface",
        "dependency_engine_interface",
    ] {
        assert!(!stdout.contains(forbidden), "forbidden={forbidden}\nstdout: {stdout}");
    }
}
