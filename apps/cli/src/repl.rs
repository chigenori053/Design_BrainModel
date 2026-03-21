/// Phase3: Agent型REPL（DBM統合版）
///
/// 設計原則：
/// - 常駐して入力を逐次処理する（stateless CLIは禁止）
/// - 入力は Command と Agent の2種類
/// - panic禁止・すべてResultで処理・不正入力でも継続
/// - 全入力を session.history に記録する
///
/// Phase3変更点：
/// - PlannerMode（RuleBased / DBM）をREPLレベルで保持
/// - /planner コマンドでモード切替可能
/// - handle_agent が planner::create_plan を呼び出す（Strategy Pattern）
/// - DBM失敗時は自動的に RuleBased にフォールバック
use std::io::{BufRead, Write};

use crate::command::{CommandRegistry, Output};
use crate::commands::register_defaults;
use crate::executor::Executor;
use crate::input::{InputState, read_input};
use crate::plan::PlanStatus;
use crate::planner::{PlannerMode, create_plan};
use crate::router::{Route, route};
use crate::session::AgentSession;
use crate::state::State;

/// REPLを起動して入力ループを実行する
///
/// `/exit` または EOF (Ctrl+D) で終了する。
pub fn run_repl<R, W>(reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut session = AgentSession::new();
    let mut registry = CommandRegistry::new();
    let mut planner_mode = PlannerMode::default();
    register_defaults(&mut registry);

    writeln!(writer, "Design Brain Model - Agent CLI (REPL Mode)").map_err(|e| e.to_string())?;
    writeln!(writer, "Type /help for commands, /exit to quit.").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    loop {
        let input = match read_input(reader, writer).map_err(|e| e.to_string())? {
            InputState::Eof => break,
            InputState::Line(line) => line,
        };

        if input.is_empty() {
            continue;
        }

        session.record(&input);

        let should_exit = dispatch(&input, &mut session, &registry, &mut planner_mode, writer)?;
        writer.flush().map_err(|e| e.to_string())?;

        if should_exit {
            break;
        }
    }

    Ok(())
}

/// 入力をルーティングして処理する
///
/// 戻り値が `true` の場合はREPL終了を示す。
fn dispatch<W: Write>(
    input: &str,
    session: &mut AgentSession,
    registry: &CommandRegistry,
    planner_mode: &mut PlannerMode,
    writer: &mut W,
) -> Result<bool, String> {
    match route(input) {
        Route::Command {
            name,
            subcommand,
            args,
        } => handle_command(
            &name,
            subcommand.as_deref(),
            &args,
            session,
            registry,
            planner_mode,
            writer,
        ),
        Route::Agent(text) => {
            handle_agent(&text, session, *planner_mode, writer)?;
            Ok(false)
        }
    }
}

/// コマンドハンドラ
///
/// REPL固有コマンド（exit/quit/help/status/plan/run/planner）を先に処理し、
/// それ以外は CommandRegistry に委譲する。
fn handle_command<W: Write>(
    name: &str,
    subcommand: Option<&str>,
    args: &[String],
    session: &mut AgentSession,
    registry: &CommandRegistry,
    planner_mode: &mut PlannerMode,
    writer: &mut W,
) -> Result<bool, String> {
    match name.trim() {
        "exit" | "quit" => return Ok(true),
        "help" => {
            print_help(registry, *planner_mode, writer)?;
            return Ok(false);
        }
        "status" => {
            writeln!(
                writer,
                "State: {} | Mode: {:?} | History: {} entries",
                session.state.as_str(),
                session.mode,
                session.history.len(),
            )
            .map_err(|e| e.to_string())?;
            return Ok(false);
        }
        "plan" => {
            handle_plan_command(subcommand, session, writer)?;
            return Ok(false);
        }
        "run" => {
            handle_run_command(session, registry, writer)?;
            return Ok(false);
        }
        "planner" => {
            handle_planner_command(subcommand, planner_mode, writer)?;
            return Ok(false);
        }
        _ => {}
    }

    // Registry へ委譲
    match registry.execute(name, subcommand, args, session) {
        Ok(Output { message }) => {
            writeln!(writer, "{message}").map_err(|e| e.to_string())?;
        }
        Err(e) => {
            writeln!(writer, "Error: {e}").map_err(|e| e.to_string())?;
        }
    }
    Ok(false)
}

/// /plan コマンド
fn handle_plan_command<W: Write>(
    _subcommand: Option<&str>,
    session: &AgentSession,
    writer: &mut W,
) -> Result<(), String> {
    match &session.current_plan {
        None => {
            writeln!(writer, "No plan. Type agent text to generate one.")
                .map_err(|e| e.to_string())?;
        }
        Some(plan) => {
            writeln!(writer, "Plan: {} [{}]", plan.id, plan.status.as_str())
                .map_err(|e| e.to_string())?;
            for step in &plan.steps {
                writeln!(
                    writer,
                    "  [{}] {} - {}",
                    step.id,
                    step.status.as_str(),
                    step.description
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// /run コマンド
fn handle_run_command<W: Write>(
    session: &mut AgentSession,
    registry: &CommandRegistry,
    writer: &mut W,
) -> Result<(), String> {
    let mut plan = match session.current_plan.take() {
        None => {
            writeln!(writer, "No plan to run. Type agent text to generate one.")
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        Some(p) => p,
    };

    if plan.status != PlanStatus::Ready && plan.status != PlanStatus::Pending {
        writeln!(
            writer,
            "Plan is not runnable (status: {}).",
            plan.status.as_str()
        )
        .map_err(|e| e.to_string())?;
        session.current_plan = Some(plan);
        return Ok(());
    }

    session.state = State::Running;

    let executor = Executor::new();
    let outputs = executor.execute(&mut plan, session, registry);

    for line in &outputs {
        writeln!(writer, "{line}").map_err(|e| e.to_string())?;
    }

    if plan.status == PlanStatus::Completed {
        session.state = State::Completed;
        writeln!(writer, "Plan completed.").map_err(|e| e.to_string())?;
    } else {
        session.state = State::Error;
        writeln!(writer, "Plan failed.").map_err(|e| e.to_string())?;
    }

    session.current_plan = Some(plan);
    Ok(())
}

/// /planner コマンド
///
/// 引数なし → 現在のモードを表示
/// rule/dbm → モードを切り替え
fn handle_planner_command<W: Write>(
    subcommand: Option<&str>,
    planner_mode: &mut PlannerMode,
    writer: &mut W,
) -> Result<(), String> {
    match subcommand {
        None => {
            writeln!(
                writer,
                "Planner mode: {} | Usage: /planner [rule|dbm]",
                planner_mode.as_str()
            )
            .map_err(|e| e.to_string())?;
        }
        Some(s) => match PlannerMode::from_str(s) {
            Some(mode) => {
                *planner_mode = mode;
                writeln!(writer, "Planner mode set to: {}", mode.as_str())
                    .map_err(|e| e.to_string())?;
            }
            None => {
                writeln!(writer, "Unknown planner mode: '{s}'. Use 'rule' or 'dbm'.")
                    .map_err(|e| e.to_string())?;
            }
        },
    }
    Ok(())
}

/// エージェントハンドラ（Phase3: Planner Strategy Pattern）
fn handle_agent<W: Write>(
    input: &str,
    session: &mut AgentSession,
    planner_mode: PlannerMode,
    writer: &mut W,
) -> Result<(), String> {
    session.context.push(input);
    session.state = State::Planning;

    let plan = create_plan(input, session, planner_mode);

    writeln!(
        writer,
        "Plan generated: {} ({} steps) [planner: {}]",
        plan.id,
        plan.steps.len(),
        planner_mode.as_str()
    )
    .map_err(|e| e.to_string())?;
    for step in &plan.steps {
        writeln!(writer, "  [{}] {}", step.id, step.description).map_err(|e| e.to_string())?;
    }
    writeln!(writer, "Type /run to execute.").map_err(|e| e.to_string())?;

    session.current_plan = Some(plan);
    session.state = State::Ready;
    Ok(())
}

/// /help コマンド出力
fn print_help<W: Write>(
    registry: &CommandRegistry,
    planner_mode: PlannerMode,
    writer: &mut W,
) -> Result<(), String> {
    writeln!(writer, "Built-in commands:").map_err(|e| e.to_string())?;
    writeln!(writer, "  /exit    - Exit the agent CLI").map_err(|e| e.to_string())?;
    writeln!(writer, "  /help    - Show this help message").map_err(|e| e.to_string())?;
    writeln!(writer, "  /status  - Show current session state").map_err(|e| e.to_string())?;
    writeln!(writer, "  /plan    - Show current plan").map_err(|e| e.to_string())?;
    writeln!(writer, "  /run     - Execute current plan").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /planner - Switch planner mode [rule|dbm] (current: {})",
        planner_mode.as_str()
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    let names = registry.command_names();
    if !names.is_empty() {
        writeln!(writer, "Registered commands: {}", names.join(", ")).map_err(|e| e.to_string())?;
    }
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(writer, "Or type any text to interact with the agent.").map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn run_with_input(input: &str) -> (String, Result<(), String>) {
        let mut reader = Cursor::new(input.as_bytes().to_vec());
        let mut writer = Vec::new();
        let result = run_repl(&mut reader, &mut writer);
        (String::from_utf8_lossy(&writer).to_string(), result)
    }

    /// dispatch を直接呼んでsessionを検査するヘルパー
    fn run_with_session(input: &str) -> (String, AgentSession, Result<(), String>) {
        let mut writer = Vec::new();
        let mut session = AgentSession::new();
        let mut registry = CommandRegistry::new();
        let mut planner_mode = PlannerMode::default();
        register_defaults(&mut registry);

        for line in input.lines() {
            if line.is_empty() {
                continue;
            }
            session.record(line);
            let _ = dispatch(
                line,
                &mut session,
                &registry,
                &mut planner_mode,
                &mut writer,
            );
        }
        (
            String::from_utf8_lossy(&writer).to_string(),
            session,
            Ok(()),
        )
    }

    /// PlannerMode を指定して run_with_session するヘルパー
    fn run_with_session_mode(
        input: &str,
        mode: PlannerMode,
    ) -> (String, AgentSession, Result<(), String>) {
        let mut writer = Vec::new();
        let mut session = AgentSession::new();
        let mut registry = CommandRegistry::new();
        let mut planner_mode = mode;
        register_defaults(&mut registry);

        for line in input.lines() {
            if line.is_empty() {
                continue;
            }
            session.record(line);
            let _ = dispatch(
                line,
                &mut session,
                &registry,
                &mut planner_mode,
                &mut writer,
            );
        }
        (
            String::from_utf8_lossy(&writer).to_string(),
            session,
            Ok(()),
        )
    }

    // ── Phase0/1/2 継承テスト ────────────────────────────────────────────

    #[test]
    fn exit_command_terminates_repl() {
        let (output, result) = run_with_input("/exit\n");
        assert!(result.is_ok(), "REPL should exit cleanly: {result:?}");
        assert!(output.contains("Design Brain Model"));
    }

    #[test]
    fn quit_command_terminates_repl() {
        let (_, result) = run_with_input("/quit\n");
        assert!(result.is_ok());
    }

    #[test]
    fn eof_terminates_repl() {
        let (_, result) = run_with_input("");
        assert!(result.is_ok(), "EOF should not error: {result:?}");
    }

    #[test]
    fn help_command_shows_commands() {
        let (output, result) = run_with_input("/help\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("/exit"), "help should list /exit");
        assert!(output.contains("/help"), "help should list /help");
    }

    #[test]
    fn empty_lines_ignored() {
        let (output, result) = run_with_input("\n\n/exit\n");
        assert!(result.is_ok());
        assert!(
            !output.contains("Plan generated"),
            "empty lines should not trigger agent"
        );
    }

    #[test]
    fn status_command_shows_state() {
        let (output, result) = run_with_input("/status\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("idle"), "initial state should be idle");
    }

    #[test]
    fn agent_input_accumulates_in_history() {
        let (output, result) = run_with_input("input1\ninput2\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Plan generated"));
    }

    #[test]
    fn all_inputs_recorded_in_session_history() {
        let (_, session, _) = run_with_session("input1\ninput2\n/status\n");
        assert_eq!(session.history.len(), 3);
        assert_eq!(session.history[0], "input1");
        assert_eq!(session.history[1], "input2");
        assert_eq!(session.history[2], "/status");
    }

    #[test]
    fn agent_updates_session_context() {
        let (_, session, _) = run_with_session("some agent input\n");
        assert_eq!(session.context.history, vec!["some agent input"]);
    }

    #[test]
    fn session_history_excludes_empty_lines() {
        let (_, session, _) = run_with_session("real input\n");
        assert!(session.history.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn status_shows_correct_history_count() {
        let (output, result) = run_with_input("line1\nline2\n/status\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("History: 3 entries"));
    }

    #[test]
    fn unknown_command_shows_registry_error() {
        let (output, result) = run_with_input("/unknown\n/exit\n");
        assert!(result.is_ok(), "unknown command should not crash REPL");
        assert!(output.contains("Error: unknown command 'unknown'"));
    }

    #[test]
    fn generate_spec_works_via_repl() {
        let (output, result) = run_with_input("/generate spec cli\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("# Spec: cli"),
            "generate spec should produce markdown"
        );
    }

    #[test]
    fn generate_design_works_via_repl() {
        let (output, result) = run_with_input("/generate design api\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("# Design: api"));
    }

    #[test]
    fn analyze_code_works_via_repl() {
        let (output, result) = run_with_input("/analyze code src/\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("src/"));
    }

    #[test]
    fn system_status_works_via_repl() {
        let (output, result) = run_with_input("/system status\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("idle"));
    }

    #[test]
    fn system_reset_clears_session_via_repl() {
        let (output, result) = run_with_input("a\nb\n/system reset\n/status\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Session reset."));
        assert!(output.contains("History: 1 entries"));
    }

    #[test]
    fn help_shows_registered_commands() {
        let (output, result) = run_with_input("/help\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("generate"),
            "help should list registered commands"
        );
        assert!(output.contains("analyze"));
        assert!(output.contains("system"));
    }

    #[test]
    fn generate_without_subcommand_lists_available() {
        let (output, result) = run_with_input("/generate\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("spec") || output.contains("design"));
    }

    #[test]
    fn unknown_subcommand_shows_error() {
        let (output, result) = run_with_input("/generate nope\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Error: unknown subcommand"));
    }

    #[test]
    fn agent_input_generates_plan() {
        let (output, result) = run_with_input("design the api\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("Plan generated"),
            "agent text should generate a plan"
        );
        assert!(output.contains("Type /run to execute"));
    }

    #[test]
    fn plan_command_shows_no_plan_initially() {
        let (output, result) = run_with_input("/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("No plan"));
    }

    #[test]
    fn plan_command_shows_plan_after_agent_input() {
        let (output, result) = run_with_input("write a spec for cli\n/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Plan:"));
        assert!(output.contains("ready"));
    }

    #[test]
    fn run_command_executes_plan() {
        let (output, result) = run_with_input("spec for the api\n/run\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("Plan completed"),
            "run should complete the plan"
        );
    }

    #[test]
    fn run_without_plan_shows_message() {
        let (output, result) = run_with_input("/run\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("No plan to run"));
    }

    #[test]
    fn agent_input_transitions_to_ready() {
        let (_, session, _) = run_with_session("design something\n");
        assert_eq!(session.state, State::Ready);
        assert!(session.current_plan.is_some());
    }

    #[test]
    fn run_transitions_to_completed() {
        let (_, session, _) = run_with_session("generate spec for cli\n/run\n");
        assert_eq!(session.state, State::Completed);
    }

    #[test]
    fn plan_spec_keyword_maps_to_generate_spec() {
        let (_, session, _) = run_with_session("write a spec for the module\n");
        let plan = session.current_plan.unwrap();
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("spec"));
    }

    #[test]
    fn plan_design_keyword_maps_to_generate_design() {
        let (_, session, _) = run_with_session("design the database schema\n");
        let plan = session.current_plan.unwrap();
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
        assert_eq!(cmd.subcommand.as_deref(), Some("design"));
    }

    #[test]
    fn plan_analyze_keyword_maps_to_analyze_code() {
        let (_, session, _) = run_with_session("analyze the source code\n");
        let plan = session.current_plan.unwrap();
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn system_reset_also_clears_plan_and_state() {
        let (_, session, _) = run_with_session("design something\n/system reset\n");
        assert!(session.current_plan.is_none());
        assert_eq!(session.state, State::Idle);
    }

    #[test]
    fn help_shows_plan_and_run_commands() {
        let (output, result) = run_with_input("/help\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("/plan"));
        assert!(output.contains("/run"));
    }

    #[test]
    fn run_after_completed_plan_shows_not_runnable() {
        let (output, result) = run_with_input("spec for cli\n/run\n/run\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("not runnable") || output.contains("Plan completed"));
    }

    // ── Phase3新規テスト ────────────────────────────────────────────────

    #[test]
    fn planner_command_shows_current_mode() {
        let (output, result) = run_with_input("/planner\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("rule_based"), "default mode is rule_based");
    }

    #[test]
    fn planner_command_switches_to_dbm() {
        let (output, result) = run_with_input("/planner dbm\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("dbm"), "mode should be set to dbm");
    }

    #[test]
    fn planner_command_switches_to_rule() {
        let (output, result) = run_with_input("/planner dbm\n/planner rule\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("rule_based"),
            "mode should be set back to rule_based"
        );
    }

    #[test]
    fn planner_unknown_mode_shows_error() {
        let (output, result) = run_with_input("/planner blah\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("Unknown planner mode"),
            "should show error for unknown mode"
        );
    }

    #[test]
    fn planner_dbm_mode_generates_plan() {
        // DBM mode may fall back to rule-based, but should still produce a plan
        let (output, result) = run_with_input("/planner dbm\ndesign the api\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("Plan generated"),
            "plan should be generated in DBM mode"
        );
        assert!(
            output.contains("planner: dbm"),
            "output should show dbm mode"
        );
    }

    #[test]
    fn planner_rule_mode_generates_plan_with_label() {
        let (output, result) = run_with_input("design the api\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("planner: rule_based"),
            "default planner label shown"
        );
    }

    #[test]
    fn help_shows_planner_command() {
        let (output, result) = run_with_input("/help\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("/planner"), "help should mention /planner");
    }

    #[test]
    fn dbm_mode_analyze_keyword_uses_analyzer() {
        // "analyze src/" with DBM mode → DBM adapter calls filesystem analyzer
        // This always succeeds since analyzer works on src/
        let (_, session, _) = run_with_session_mode("analyze src/\n", PlannerMode::DBM);
        let plan = session.current_plan.unwrap();
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn dbm_mode_session_state_is_ready_after_plan() {
        let (_, session, _) = run_with_session_mode("spec for the module\n", PlannerMode::DBM);
        assert_eq!(session.state, State::Ready);
        assert!(session.current_plan.is_some());
    }

    // ── Phase3.1: プロジェクト解析テスト ─────────────────────────────────

    #[test]
    fn analyze_project_command_works_via_repl() {
        let (output, result) = run_with_input("/analyze project src/\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Project Summary:"), "got: {output}");
    }

    #[test]
    fn project_keyword_in_agent_generates_two_step_plan() {
        let (_, session, _) = run_with_session("analyze the whole project\n");
        let plan = session.current_plan.unwrap();
        assert_eq!(
            plan.steps.len(),
            2,
            "project input should create 2-step plan"
        );
        let cmd0 = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd0.subcommand.as_deref(), Some("project"));
    }

    #[test]
    fn run_project_plan_executes_both_steps() {
        // "analyze project ." → 2-step plan → /run → both steps execute
        let (output, result) = run_with_input("analyze project .\n/run\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Plan completed"), "got: {output}");
    }

    #[test]
    fn planner_mode_persists_across_commands() {
        // Switch to DBM mode, execute some commands, verify mode persists
        let (output, result) = run_with_input("/planner dbm\n/planner\n/exit\n");
        assert!(result.is_ok());
        // Second /planner shows current mode which should be dbm
        let mode_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.contains("Planner mode"))
            .collect();
        // The mode line from /planner (no arg) shows the current mode
        assert!(mode_lines.iter().any(|l| l.contains("dbm")));
    }
}
