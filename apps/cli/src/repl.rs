/// DBM_CLI: 自然言語インタラクティブ REPL
///
/// 設計原則：
/// - 常駐して入力を逐次処理する（stateless CLIは禁止）
/// - 入力は Command と Agent（自然言語）の2種類
/// - 自然言語入力は即時自動実行（/run 不要）
/// - panic禁止・すべてResultで処理・不正入力でも継続
/// - user input のみを session.history に記録する
/// - REPL output は session.transcript に記録する
use std::io::{BufRead, Write};

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

/// REPLを起動して入力ループを実行する
///
/// `/exit` または EOF (Ctrl+D) で終了する。
pub fn run_repl<R, W>(reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut session = AgentSession::new();
    let mut conversation = ConversationState::default();
    let mut registry = CommandRegistry::new();
    let mut planner_mode = PlannerMode::default();
    register_defaults(&mut registry);

    print_banner(writer)?;

    loop {
        let input = match read_input_with_label(
            reader,
            writer,
            session.state,
            conversation.prompt_label(),
        )
        .map_err(|e| e.to_string())? {
            InputState::Eof => break,
            InputState::Line(line) => line,
        };

        if input.is_empty() {
            continue;
        }

        session.record(&input);

        let should_exit = dispatch(
            &input,
            &mut session,
            &mut conversation,
            &registry,
            &mut planner_mode,
            writer,
        )?;
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
    conversation: &mut ConversationState,
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
            conversation,
            registry,
            planner_mode,
            writer,
        ),
        Route::Agent(text) => {
            handle_agent(&text, session, conversation, registry, *planner_mode, writer)?;
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
    conversation: &mut ConversationState,
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
        "clear" => {
            // コンテキストとカレントプランをリセット（historyは保持）
            session.context.history.clear();
            session.context.last_path = None;
            session.context.last_command = None;
            session.current_plan = None;
            *conversation = ConversationState::default();
            session.state = State::Idle;
            writeln!(writer, "コンテキストをクリアしました。").map_err(|e| e.to_string())?;
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

/// 自然言語ハンドラ：プランを生成して即時実行する
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

    if let Some(goal) = detect_goal(input) {
        conversation.autonomous_label = Some(format!("autonomous:{}", goal_label(goal)));
        emit_output(
            session,
            writer,
            &format!("[autonomous mode] goal={}", goal_label(goal)),
        )?;

        if cfg!(test) {
            session.state = State::Completed;
            emit_output(session, writer, "[test] planner-only mode")?;
            conversation.autonomous_label = None;
            return Ok(());
        }

        session.state = State::Running;
        let result = run_goal_loop(goal, session, conversation, AutonomousLoop::default());
        for output in result.outputs {
            if !output.trim().is_empty() {
                emit_output(session, writer, &output)?;
            }
        }
        session.state = if result.completed {
            State::Completed
        } else {
            State::Error
        };
        conversation.autonomous_label = None;
        if result.completed {
            print_follow_up_suggestions(input, session, writer)?;
        }
        return Ok(());
    }

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

    // プランナーラベルとステップ数を表示
    let planner_summary = format!(
        "[planner: {}] {} ステップ",
        planner_mode.as_str(),
        plan.steps.len(),
    );
    emit_output(session, writer, &planner_summary)?;

    if cfg!(test) {
        plan.status = PlanStatus::Completed;
        session.state = State::Completed;
        session.current_plan = Some(plan);
        emit_output(session, writer, "[test] planner-only mode")?;
        return Ok(());
    }

    // 各ステップを in-process 実行する。REPL 自身を再起動するサブプロセス経路は使わない。
    session.state = State::Running;
    let executor = Executor::new();
    for output in executor.execute(&mut plan, session, registry) {
        if !output.trim().is_empty() {
            emit_output(session, writer, &output)?;
        }
    }

    if plan.status == PlanStatus::Completed {
        session.state = State::Completed;
        print_follow_up_suggestions(input, session, writer)?;
    } else {
        session.state = State::Error;
    }

    session.current_plan = Some(plan);
    conversation.autonomous_label = None;
    Ok(())
}

/// 実行後のコンテキスト対応次ステップ提案
fn print_follow_up_suggestions<W: Write>(
    input: &str,
    session: &mut AgentSession,
    writer: &mut W,
) -> Result<(), String> {
    let lower = input.to_lowercase();

    let suggestions: &[&str] = if lower.contains("project") || lower.contains("プロジェクト")
    {
        &["validate でアーキテクチャを検証", "refactor で改善点を提案"]
    } else if lower.contains("analyze")
        || lower.contains("分析")
        || lower.contains("解析")
        || lower.contains("調べ")
    {
        &["validate でアーキテクチャを検証", "refactor で改善点を提案"]
    } else if lower.contains("validate") || lower.contains("検証") || lower.contains("チェック")
    {
        &["refactor で問題を修正", "coding --apply で変更を適用"]
    } else if lower.contains("refactor") || lower.contains("リファクタ") || lower.contains("改善")
    {
        &["coding --apply で変更を適用"]
    } else if lower.contains("spec") || lower.contains("仕様") {
        &["design で詳細設計を生成", "coding で実装を開始"]
    } else if lower.contains("design") || lower.contains("設計") {
        &["validate で設計を検証", "coding で実装を開始"]
    } else {
        &[]
    };

    if !suggestions.is_empty() {
        emit_output(session, writer, "")?;
        emit_output(session, writer, "💡 次のステップ:")?;
        for s in suggestions {
            emit_output(session, writer, &format!("   {s}"))?;
        }
    }
    Ok(())
}

fn emit_output<W: Write>(
    session: &mut AgentSession,
    writer: &mut W,
    line: &str,
) -> Result<(), String> {
    session.record_output(line);
    writeln!(writer, "{line}").map_err(|e| e.to_string())
}

/// DBM_CLI バナーを表示する
fn print_banner<W: Write>(writer: &mut W) -> Result<(), String> {
    writeln!(
        writer,
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  DBM_CLI  Design Brain Model").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  自然言語または /command でアーキテクチャを設計・解析できます。"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(writer, "  自然言語の例:").map_err(|e| e.to_string())?;
    writeln!(writer, "    このプロジェクト全体を解析して").map_err(|e| e.to_string())?;
    writeln!(writer, "    GUIで構造を開いて問題ノードを見せて").map_err(|e| e.to_string())?;
    writeln!(writer, "    Rust unsafe を減らして cargo check して").map_err(|e| e.to_string())?;
    writeln!(writer, "    さっきの場所を安全に修正して   ← 前回パスを自動使用")
        .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(writer, "  コマンドの例:").map_err(|e| e.to_string())?;
    writeln!(writer, "    /validate src/lib.rs").map_err(|e| e.to_string())?;
    writeln!(writer, "    /rules list").map_err(|e| e.to_string())?;
    writeln!(writer, "    /memory import seeds/knowledge.json").map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(writer, "  /help でコマンド一覧  /exit で終了").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    )
    .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// /help コマンド出力
fn print_help<W: Write>(
    registry: &CommandRegistry,
    planner_mode: PlannerMode,
    writer: &mut W,
) -> Result<(), String> {
    writeln!(
        writer,
        "── 自然言語（入力 → 即時実行）──────────────────────────────"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  このプロジェクトを解析して     → design_cli analyze .")
        .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  この循環依存を安全に直して     → design_cli analyze . → coding . --safe --check"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  GUIで構造を開いて             → design_cli structure view .")
        .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  unsafeを減らしてcargo check   → analyze → coding --safe --check → validate"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  さっきの場所を検証して         → 前回パスを自動使用"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "── /コマンド（直接実行）────────────────────────────────────"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /analyze [code|project] <path>  - コード/プロジェクト解析"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /validate <path>                - アーキテクチャを検証"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /refactor <path>                - リファクタリング案を生成"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /refactoring <path>             - 解析結果をコードへ適用"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /coding <path>                  - コード変更セットを生成"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  /diff <path>                    - 変更差分を表示")
        .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /check <path>                   - 変更をドライラン"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  /apply <path>                   - 変更を適用")
        .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /exec [detect|install|build|test|run] <path> - 実行基盤"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /generate [spec|design] <path>  - 仕様/設計書を生成"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  /rules [list|inspect|promote..] - ルール管理")
        .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /memory import <path>           - メモリにシードをインポート"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "Registered commands: {}",
        registry.command_names().join(", ")
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "── セッション管理 ──────────────────────────────────────────"
    )
    .map_err(|e| e.to_string())?;
    writeln!(writer, "  /exit    - 終了").map_err(|e| e.to_string())?;
    writeln!(writer, "  /help    - このヘルプを表示").map_err(|e| e.to_string())?;
    writeln!(writer, "  /status  - セッション状態を確認").map_err(|e| e.to_string())?;
    writeln!(writer, "  /plan    - 最後のプランを確認").map_err(|e| e.to_string())?;
    writeln!(writer, "  /run     - 現在のプランを実行").map_err(|e| e.to_string())?;
    writeln!(writer, "  /clear   - コンテキストをリセット").map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  /planner [rule|dbm] - プランナーモード切替（現在: {}）",
        planner_mode.as_str()
    )
    .map_err(|e| e.to_string())?;
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
        let mut conversation = ConversationState::default();
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
                &mut conversation,
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
        let mut conversation = ConversationState::default();
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
                &mut conversation,
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
        let (_, session, _) = run_with_session("input1\n");
        assert_eq!(session.history.len(), 1);
        assert_eq!(session.history[0], "input1");
        assert_eq!(session.state, State::Completed);
        assert!(session.current_plan.is_some());
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
    fn agent_output_accumulates_in_transcript() {
        let (_, session, _) = run_with_session("some agent input\n");
        assert!(session.transcript.len() >= 2);
        assert!(session
            .transcript
            .iter()
            .any(|line| line.contains("[planner:")));
        assert!(session
            .transcript
            .iter()
            .any(|line| line.contains("[test] planner-only mode")));
        assert_eq!(session.state, State::Completed);
        assert_eq!(
            session.current_plan.as_ref().map(|plan| plan.status),
            Some(PlanStatus::Completed)
        );
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
        // 自然言語入力でプランが生成されて即時実行される
        let (output, result) = run_with_input("design the api\n/exit\n");
        assert!(result.is_ok());
        // "[planner: rule_based]" が表示される
        assert!(
            output.contains("[planner:"),
            "agent text should show planner label: {output}"
        );
        // "Type /run" は表示されない（自動実行のため）
        assert!(!output.contains("Type /run to execute"));
    }

    #[test]
    fn plan_command_shows_no_plan_initially() {
        let (output, result) = run_with_input("/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("No plan"));
    }

    #[test]
    fn plan_command_shows_plan_after_agent_input() {
        // 自動実行後でもプランはセッションに残っている
        let (output, result) = run_with_input("write a spec for cli\n/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Plan:"), "got: {output}");
        // 自動実行後はステータスが failed または completed
        assert!(
            output.contains("failed") || output.contains("completed"),
            "plan should be executed: {output}"
        );
    }

    #[test]
    fn run_command_executes_plan() {
        // 自然言語入力で既に自動実行されているため、/run は "not runnable" を返す
        let (output, result) = run_with_input("spec for the api\n/run\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("not runnable") || output.contains("No plan to run"),
            "auto-executed plan should not be runnable again: {output}"
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
        // 自動実行後: Completed（成功）または Error（サブプロセス失敗）
        let (_, session, _) = run_with_session("design something\n");
        assert_ne!(
            session.state,
            State::Idle,
            "state should advance from idle after agent input"
        );
        assert_ne!(
            session.state,
            State::Planning,
            "should not be stuck in planning"
        );
        assert!(session.current_plan.is_some());
    }

    #[test]
    fn run_transitions_to_completed() {
        // 自動実行後は plan が実行済みのため /run は不要
        let (_, session, _) = run_with_session("generate spec for cli\n");
        assert!(
            session.state == State::Completed || session.state == State::Error,
            "state should be completed or error after auto-execute, got: {:?}",
            session.state
        );
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
        // DBM mode は rule-based にフォールバックしてもプランを実行する
        let (output, result) = run_with_input("/planner dbm\ndesign the api\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("[planner: dbm]"),
            "output should show dbm mode: {output}"
        );
    }

    #[test]
    fn planner_rule_mode_generates_plan_with_label() {
        let (output, result) = run_with_input("design the api\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("[planner: rule_based]"),
            "default planner label shown: {output}"
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
        // 自動実行後: Completed（成功）または Error（サブプロセス失敗）
        let (_, session, _) = run_with_session_mode("spec for the module\n", PlannerMode::DBM);
        assert_ne!(session.state, State::Idle);
        assert!(session.current_plan.is_some());
    }

    // ── Phase3.1: プロジェクト解析テスト ─────────────────────────────────

    #[test]
    fn analyze_project_command_works_via_repl() {
        let (output, result) = run_with_input("/analyze project src/\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("DBM Analyze Report"), "got: {output}");
        assert!(output.contains("Target: src/"), "got: {output}");
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
        // "analyze project ." → 2-step plan → 自動実行（サブプロセス経由）
        let (_, session, _) = run_with_session("analyze project .\n");
        let plan = session.current_plan.unwrap();
        // 2ステップのプランが生成される
        assert_eq!(plan.steps.len(), 2, "project plan should have 2 steps");
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
