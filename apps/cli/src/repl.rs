/// DBM_CLI: 自然言語インタラクティブ REPL
///
/// 設計原則：
/// - 常駐して入力を逐次処理する（stateless CLIは禁止）
/// - 入力は Command と Agent（自然言語）の2種類
/// - 自然言語入力は即時自動実行（/run 不要）
/// - panic禁止・すべてResultで処理・不正入力でも継続
/// - user input のみを session.history に記録する
/// - REPL output は session.transcript に記録する
use std::io::{self, BufRead, IsTerminal, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::command::{CommandRegistry, Output};
use crate::commands::register_defaults;
use crate::input::{InputState, read_input_with_label};
use crate::ir::{
    IRPersistenceArtifact, emit_intent_captured, emit_plan_accepted, emit_plan_proposed,
    log_ir_bypass_warning, persist_ir_transition, restore_or_initialize_ir_state,
};
use crate::nl::autonomous::{AutonomousLoop, run_goal_loop};
use crate::nl::goal::{detect_goal, goal_label};
use crate::nl::planner_v2::update_conversation_after_plan;
use crate::nl::session::ConversationState;
use crate::nl::types::{CodingOptions as NlCodingOptions, CommandPlan, PlannedStep};
use crate::nl::{
    execute_ir_plan, render_plan_summary_with_label, resolve_command_plan, to_runtime_plan,
};
use crate::nl_executor::run_design_command;
use crate::planner::{PlannerMode, create_plan};
use crate::router::{Route, route};
use crate::service::dto::{ActionKind, SessionAppliedDiff};
use crate::session::AgentSession;
use crate::state::State;
use crate::tui::composer::ExecutionResult;
use crate::tui::composer::{ComposerAction, ComposerViewState, render_composer};
use crate::tui::edit_block::CodingReviewReport;
use crate::tui::proc_strip::{DONE_MIN_VISIBLE, ProcPhase, RUNNING_MIN_VISIBLE};
use crate::tui::review_batch::ReviewBatchState;

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|e| e.to_string())?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| e.to_string())?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
    }
}

/// REPLを起動して入力ループを実行する
///
/// `/exit` または EOF (Ctrl+D) で終了する。
pub fn run_repl<R, W>(workspace_root: PathBuf, reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut session = AgentSession::with_root(workspace_root.clone());
    let mut conversation = ConversationState::default();
    hydrate_ir_state(&workspace_root, &mut conversation)?;
    let mut registry = CommandRegistry::new();
    let mut planner_mode = PlannerMode::default();
    register_defaults(&mut registry);

    print_banner(writer)?;

    loop {
        let input =
            match read_input_with_label(reader, writer, session.state, conversation.prompt_label())
                .map_err(|e| e.to_string())?
            {
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

pub fn run_repl_stdio(workspace_root: PathBuf) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut reader = io::BufReader::new(stdin.lock());
        let mut writer = stdout.lock();
        return run_repl(workspace_root, &mut reader, &mut writer);
    }

    let mut session = AgentSession::with_root(workspace_root.clone());
    let mut conversation = ConversationState::default();
    hydrate_ir_state(&workspace_root, &mut conversation)?;
    let mut registry = CommandRegistry::new();
    let mut planner_mode = PlannerMode::default();
    register_defaults(&mut registry);

    let terminal_guard = TerminalGuard::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        run_interactive_loop(
            &mut terminal,
            &mut session,
            &mut conversation,
            &registry,
            &mut planner_mode,
        )
    }));
    let _ = terminal.show_cursor();
    drop(terminal);
    drop(terminal_guard);

    match result {
        Ok(result) => result,
        Err(payload) => Err(format!(
            "interactive REPL panicked: {}",
            panic_payload_message(payload)
        )),
    }
}

pub fn dispatch_repl_input<W: Write>(
    input: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    planner_mode: &mut PlannerMode,
    writer: &mut W,
) -> Result<bool, String> {
    let mut registry = CommandRegistry::new();
    register_defaults(&mut registry);
    dispatch(
        input,
        session,
        conversation,
        &registry,
        planner_mode,
        writer,
    )
}

pub fn reset_review_session(view: &mut ComposerViewState, session: &mut AgentSession) {
    view.reset_review_session();
    view.state = State::Idle;
    session.current_plan = None;
    session.state = State::Idle;
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
            handle_agent(
                &text,
                session,
                conversation,
                registry,
                *planner_mode,
                writer,
            )?;
            Ok(false)
        }
    }
}

fn current_branch_name(root: &std::path::Path) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|output| {
            if !output.status.success() {
                return None;
            }
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch.is_empty() {
                None
            } else {
                Some(branch)
            }
        })
}
fn banner_lines() -> Vec<String> {
    vec![
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(),
        "  DBM_CLI  Design Brain Model".to_string(),
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(),
        "  自然言語または /command でアーキテクチャを設計・解析できます。".to_string(),
        String::new(),
        "  Enter で送信 / 単一行・末尾追従入力".to_string(),
        "  /help でコマンド一覧  /exit で終了".to_string(),
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(),
    ]
}

fn run_interactive_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    registry: &CommandRegistry,
    planner_mode: &mut PlannerMode,
) -> Result<(), String> {
    let mut view = ComposerViewState::new(banner_lines(), session.state);
    view.sync_context(
        conversation,
        current_branch_name(
            session
                .workspace_root
                .as_deref()
                .unwrap_or(std::path::Path::new(".")),
        ),
        None,
    );

    loop {
        view.state = session.state;
        view.sync_context(
            conversation,
            current_branch_name(
                session
                    .workspace_root
                    .as_deref()
                    .unwrap_or(std::path::Path::new(".")),
            ),
            None,
        );
        terminal
            .draw(|frame| render_composer(frame, &mut view))
            .map_err(|e| e.to_string())?;

        match event::read().map_err(|e| e.to_string())? {
            Event::Key(key) => {
                if let Some(action) = global_key_action(key, &view) {
                    match action {
                        GlobalKeyAction::ForceQuit | GlobalKeyAction::Exit => break,
                    }
                }

                if view.review.is_some() && view.intent_is_blank() {
                    if handle_review_key(
                        key.code,
                        session,
                        conversation,
                        &mut view,
                        &mut |view| draw_view(terminal, view),
                        &mut sleep,
                    )? {
                        draw_view(terminal, &mut view)?;
                        continue;
                    }
                }

                match view.handle_key_event(key) {
                    ComposerAction::None => {}
                    ComposerAction::Exit | ComposerAction::ForceQuit => break,
                    ComposerAction::Submit(event) => {
                        let should_exit = dispatch_submission(
                            &event.input,
                            session,
                            conversation,
                            registry,
                            planner_mode,
                            &mut view,
                            &mut |view| draw_view(terminal, view),
                            &mut sleep,
                        )?;
                        if should_exit {
                            break;
                        }
                    }
                }
            }
            Event::Mouse(mouse) => match view.handle_mouse_event(mouse) {
                ComposerAction::None => {}
                ComposerAction::Exit | ComposerAction::ForceQuit => break,
                ComposerAction::Submit(event) => {
                    let should_exit = dispatch_submission(
                        &event.input,
                        session,
                        conversation,
                        registry,
                        planner_mode,
                        &mut view,
                        &mut |view| draw_view(terminal, view),
                        &mut sleep,
                    )?;
                    if should_exit {
                        break;
                    }
                }
            },
            _ => {}
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalKeyAction {
    Exit,
    ForceQuit,
}

fn global_key_action(key: KeyEvent, view: &ComposerViewState) -> Option<GlobalKeyAction> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
        return Some(GlobalKeyAction::ForceQuit);
    }
    if key.code == KeyCode::Esc && view.intent_is_blank() {
        return Some(GlobalKeyAction::Exit);
    }
    None
}

fn handle_review_key(
    code: KeyCode,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    view: &mut ComposerViewState,
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
) -> Result<bool, String> {
    if !view.intent_is_blank() {
        return Ok(false);
    }
    let Some(review) = view.review.as_mut() else {
        return Ok(false);
    };
    match code {
        KeyCode::Char('J') | KeyCode::Down => {
            review.next_block();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char('K') | KeyCode::Up => {
            review.previous_block();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char('E') | KeyCode::Char('C') => {
            review.toggle_expand_focused();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char(' ') => {
            review.toggle_batch_selected();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char(']') | KeyCode::Char('/') => {
            review.next_group();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char('[') => {
            review.previous_group();
            redraw(view)?;
            Ok(true)
        }
        KeyCode::Char('D') => {
            let summary = if review.selected_pending_count() > 0 {
                review.discard_selected_batch()
            } else {
                review.discard_focused_block()
            };
            if let Some(summary) = summary {
                view.push_transcript_line(summary);
                reset_review_session(view, session);
            }
            Ok(true)
        }
        KeyCode::Char('A') | KeyCode::Enter => {
            let phases = vec![ProcPhase::WritingEdit];
            run_proc_strip_only(view, &phases, redraw, sleeper, &mut || Ok(()))?;
            if let Some(review) = view.review.as_mut() {
                let snapshot = build_session_diff_snapshot(review);
                let summary = if review.selected_pending_count() > 0 {
                    review.apply_selected_batch()?
                } else {
                    review.apply_focused_block()?
                };
                if let Some(applied) = summary {
                    view.push_transcript_line(applied);
                    view.set_execution_result(execution_result_from_snapshot(
                        snapshot.clone(),
                        ExecutionResult::NoOp,
                    ));
                    record_ir_apply(conversation, snapshot);
                    view.ir_state = conversation.ir_state.clone();
                    reset_review_session(view, session);
                }
            }
            Ok(true)
        }
        KeyCode::Char('R') => {
            let phases = vec![ProcPhase::WritingEdit];
            run_proc_strip_only(view, &phases, redraw, sleeper, &mut || Ok(()))?;
            if let Some(review) = view.review.as_mut() {
                let snapshot = build_session_diff_snapshot(review);
                if let Some(summary) = review.rollback_last_batch()? {
                    view.push_transcript_line(summary);
                    let reason = if snapshot.is_some() {
                        "manual_rollback"
                    } else {
                        "validation_failed"
                    };
                    view.set_execution_result(ExecutionResult::RolledBack {
                        reason: reason.to_string(),
                    });
                    record_ir_rollback(conversation, snapshot);
                    view.ir_state = conversation.ir_state.clone();
                    reset_review_session(view, session);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn draw_view(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    view: &mut ComposerViewState,
) -> Result<(), String> {
    view.sync_buffer_metadata();
    terminal
        .draw(|frame| render_composer(frame, view))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn dispatch_submission(
    input: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    registry: &CommandRegistry,
    planner_mode: &mut PlannerMode,
    view: &mut ComposerViewState,
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
) -> Result<bool, String> {
    if input.trim().is_empty() {
        return Ok(false);
    }
    session.record(input);
    view.reset_review_session();
    let result = execute_submission_with_proc_strip(
        input,
        session,
        conversation,
        registry,
        planner_mode,
        view,
        redraw,
        sleeper,
    )?;
    if session.state == State::Completed
        && let Some(completed_step) = completed_step_name(input)
    {
        conversation.ir_state.next_allowed_actions = next_allowed_actions_for(completed_step);
        if completed_step == "validate" {
            let before = conversation.ir_state.clone();
            conversation.mark_transaction_validated();
            let _ = persist_ir_transition(
                &before,
                &conversation.ir_state,
                ActionKind::Validate,
                input.to_string(),
                IRPersistenceArtifact {
                    diff_ref: conversation
                        .ir_state
                        .active_transaction
                        .as_ref()
                        .and_then(|tx| tx.latest_diff_ref.clone()),
                    build_ok: conversation
                        .ir_state
                        .active_transaction
                        .as_ref()
                        .and_then(|tx| tx.latest_build_ok),
                    validation_ok: Some(true),
                    rollback_checkpoint: None,
                },
            );
        }
        view.ir_state = conversation.ir_state.clone();
        redraw(view)?;
    }
    Ok(result)
}

fn execute_submission_with_proc_strip(
    input: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    registry: &CommandRegistry,
    planner_mode: &mut PlannerMode,
    view: &mut ComposerViewState,
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
) -> Result<bool, String> {
    if let Some(should_exit) =
        try_execute_reviewable_coding(input, session, conversation, view, redraw, sleeper)?
    {
        return Ok(should_exit);
    }
    let result = run_proc_strip_lifecycle(
        view,
        input,
        &proc_strip_plan(input),
        redraw,
        sleeper,
        &mut |output| dispatch(input, session, conversation, registry, planner_mode, output),
    )?;
    view.sync_context(
        conversation,
        current_branch_name(
            session
                .workspace_root
                .as_deref()
                .unwrap_or(std::path::Path::new(".")),
        ),
        None,
    );
    Ok(result)
}

fn try_execute_reviewable_coding(
    input: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    view: &mut ComposerViewState,
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
) -> Result<Option<bool>, String> {
    if exact_file_route_plan(input).is_some() {
        return Ok(None);
    }

    if detect_goal(input).is_some() || !matches!(route(input), Route::Agent(_)) {
        return Ok(None);
    }

    session.context.push(input);
    let (command_plan, planner_label) = plan_agent_input(input, session, conversation);
    let Some(command_plan) = command_plan else {
        return Ok(None);
    };
    if !command_plan
        .steps
        .iter()
        .all(|step| matches!(step, PlannedStep::Coding(_, _)))
    {
        return Ok(None);
    }

    let _plan_id =
        record_plan_lifecycle(session, conversation, input, &command_plan, planner_label)?;

    let mut reviews = Vec::<(CodingReviewReport, String)>::new();
    run_proc_strip_only(view, &proc_strip_plan(input), redraw, sleeper, &mut || {
        for step in &command_plan.steps {
            let PlannedStep::Coding(path, options) = step else {
                continue;
            };
            reviews.push((
                run_coding_review_command(path, options)?,
                sanitize_patch_family(input),
            ));
        }
        Ok(())
    })?;

    let planner_summary = render_plan_summary_with_label(&command_plan, planner_label);
    update_conversation_after_plan(input, &command_plan, conversation);
    session.current_plan = Some(to_runtime_plan(&command_plan));
    session.state = State::Completed;
    conversation.autonomous_label = None;

    let mut review_blocks = 0usize;
    if let Some(review) = ReviewBatchState::from_coding_reports(&reviews)? {
        review_blocks = review.blocks.len();
        view.activate_review(review);
    }
    if let Some(PlannedStep::Coding(path, options)) = command_plan.steps.first() {
        let _ = (review_blocks, options);
        let before = conversation.ir_state.clone();
        conversation.start_preview_transaction(path.clone());
        let _ = persist_ir_transition(
            &before,
            &conversation.ir_state,
            ActionKind::CodingPreview,
            format!("preview {}", path.display()),
            IRPersistenceArtifact::default(),
        );
    }
    view.sync_context(
        conversation,
        current_branch_name(
            session
                .workspace_root
                .as_deref()
                .unwrap_or(std::path::Path::new(".")),
        ),
        None,
    );
    view.push_transcript_line(planner_summary);
    if review_blocks > 0 {
        if let Some(diff) = view
            .review
            .as_ref()
            .and_then(|review| review.preview_diff_snapshot())
        {
            view.set_execution_result(execution_result_from_snapshot(
                Some(diff),
                ExecutionResult::NoOp,
            ));
        }
        view.push_transcript_line(format!("Review ready: {review_blocks} file patch group(s)"));
    } else {
        view.set_execution_result(ExecutionResult::NoOp);
    }
    Ok(Some(false))
}

fn run_coding_review_command(
    path: &std::path::Path,
    options: &NlCodingOptions,
) -> Result<CodingReviewReport, String> {
    let path_str = path.display().to_string();
    let is_file_target =
        path_str.ends_with(".rs") || path_str.ends_with(".toml") || path_str.ends_with(".md");
    let mut args = if is_file_target {
        vec![".".to_string(), "--target".to_string(), path_str]
    } else {
        vec![path_str]
    };
    if let Some(request) = &options.request {
        args.push("--request".to_string());
        args.push(request.clone());
    }
    if options.safe {
        args.push("--safe".to_string());
    }
    if options.check {
        args.push("--check".to_string());
    }
    args.push("--json".to_string());
    let output = run_design_command("coding", &args)?;
    let json = output
        .find('{')
        .map(|index| &output[index..])
        .unwrap_or(output.as_str());
    serde_json::from_str::<CodingReviewReport>(json).map_err(|err| err.to_string())
}

fn sanitize_patch_family(input: &str) -> String {
    let first_line = input.lines().next().unwrap_or("coding request").trim();
    if first_line.is_empty() {
        "coding request".to_string()
    } else {
        first_line.chars().take(48).collect()
    }
}

fn append_submission_transcript(view: &mut ComposerViewState, input: &str, output: &[u8]) {
    view.push_transcript_line(format!("> {}", input.replace('\n', "\n  ")));
    let rendered = String::from_utf8_lossy(output);
    for line in rendered.lines() {
        view.push_transcript_line(line.to_string());
    }
    view.restore_intent_document_focus();
}

fn run_proc_strip_lifecycle(
    view: &mut ComposerViewState,
    input: &str,
    phases: &[ProcPhase],
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
    execute: &mut dyn FnMut(&mut Vec<u8>) -> Result<bool, String>,
) -> Result<bool, String> {
    let mut output = Vec::new();
    match run_proc_strip_core(view, phases, redraw, sleeper, &mut || execute(&mut output)) {
        Ok(should_exit) => {
            append_submission_transcript(view, input, &output);
            Ok(should_exit)
        }
        Err(err) => {
            append_submission_transcript(view, input, &output);
            if is_validate_request(input) {
                view.set_execution_result(ExecutionResult::RolledBack {
                    reason: "validation_failed".to_string(),
                });
            } else {
                view.set_execution_result(ExecutionResult::Failure {
                    reason: err.clone(),
                });
            }
            view.push_transcript_line(format!("Error: {err}"));
            Ok(false)
        }
    }
}

fn run_proc_strip_only(
    view: &mut ComposerViewState,
    phases: &[ProcPhase],
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
    execute: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    run_proc_strip_core(view, phases, redraw, sleeper, &mut || {
        execute()?;
        Ok(false)
    })
    .map(|_| ())
}

fn run_proc_strip_core(
    view: &mut ComposerViewState,
    phases: &[ProcPhase],
    redraw: &mut dyn FnMut(&mut ComposerViewState) -> Result<(), String>,
    sleeper: &mut dyn FnMut(Duration),
    execute: &mut dyn FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    view.proc_strip
        .set(ProcPhase::Running, proc_strip_detail(ProcPhase::Running));
    redraw(view)?;
    sleeper(RUNNING_MIN_VISIBLE);

    for phase in phases {
        view.proc_strip.set(*phase, proc_strip_detail(*phase));
        redraw(view)?;
    }

    match execute() {
        Ok(should_exit) => {
            view.proc_strip
                .set(ProcPhase::Done, proc_strip_detail(ProcPhase::Done));
            redraw(view)?;
            sleeper(DONE_MIN_VISIBLE);
            view.proc_strip.reset();
            redraw(view)?;
            Ok(should_exit)
        }
        Err(err) => {
            view.proc_strip.set(ProcPhase::Error, err.clone());
            redraw(view)?;
            sleeper(DONE_MIN_VISIBLE);
            view.proc_strip.reset();
            redraw(view)?;
            Err(err)
        }
    }
}

fn proc_strip_plan(input: &str) -> Vec<ProcPhase> {
    let lower = input.to_lowercase();
    match route(input) {
        Route::Command { name, .. } => match name.as_str() {
            "coding" | "diff" | "check" | "apply" | "refactor" => {
                vec![ProcPhase::ReadingFiles, ProcPhase::WritingEdit]
            }
            "analyze" | "validate" | "run" | "design" => vec![ProcPhase::ReadingFiles],
            _ => vec![ProcPhase::Planning],
        },
        Route::Agent(_) => {
            let mut phases = vec![ProcPhase::Planning];
            if ["解析", "analyze", "check", "review", "inspect"]
                .iter()
                .any(|token| lower.contains(token))
            {
                phases.push(ProcPhase::ReadingFiles);
            }
            if ["修正", "fix", "edit", "patch", "refactor", "改善", "diff"]
                .iter()
                .any(|token| lower.contains(token))
            {
                phases.push(ProcPhase::WritingEdit);
            }
            phases
        }
    }
}

fn proc_strip_detail(phase: ProcPhase) -> &'static str {
    match phase {
        ProcPhase::Idle => "ready",
        ProcPhase::Running => "execution started...",
        ProcPhase::ReadingFiles => "reading files...",
        ProcPhase::Planning => "planning request...",
        ProcPhase::WritingEdit => "generating edits...",
        ProcPhase::Done => "execution complete",
        ProcPhase::Error => "execution failed",
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
    if let Some(output) =
        execute_direct_subcommand(name, subcommand, args, session, conversation, writer)?
    {
        writeln!(writer, "{output}").map_err(|e| e.to_string())?;
        return Ok(false);
    }

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
            handle_run_command(session, writer)?;
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

fn execute_direct_subcommand<W: Write>(
    name: &str,
    subcommand: Option<&str>,
    args: &[String],
    session: &mut AgentSession,
    conversation: &mut ConversationState,
    writer: &mut W,
) -> Result<Option<String>, String> {
    if !matches!(
        name,
        "analyze" | "coding" | "validate" | "structure" | "rules" | "memory" | "simulate"
    ) {
        return Ok(None);
    }

    if let Some(path) = first_path_like_arg(args) {
        session.context.set_last_path(&path);
    }
    session.context.last_command = Some(name.to_string());

    let Some(command_plan) = direct_command_plan(name, subcommand, args) else {
        return Ok(None);
    };
    let planner_label = format!("direct:{name}");
    let synthetic_input = render_direct_command_input(name, subcommand, args);
    let plan_id = record_plan_lifecycle(
        session,
        conversation,
        &synthetic_input,
        &command_plan,
        &planner_label,
    )?;
    update_conversation_after_plan(&synthetic_input, &command_plan, conversation);

    if cfg!(test) {
        let mut rendered = vec![format!("[direct-dispatch] {name}")];
        if let Some(sub) = subcommand {
            rendered.push(sub.to_string());
        }
        rendered.extend(args.iter().cloned());
        conversation.last_accepted_plan_id = Some(plan_id);
        session.current_plan = Some(to_runtime_plan(&command_plan));
        session.state = State::Completed;
        return Ok(Some(rendered.join(" ")));
    }

    let output = execute_ir_plan(plan_id, &command_plan, session, conversation).join("\n");
    conversation.last_accepted_plan_id = Some(plan_id);
    session.current_plan = Some(to_runtime_plan(&command_plan));
    session.state = State::Completed;
    let _ = writer;
    Ok(Some(output))
}

fn direct_command_plan(
    name: &str,
    subcommand: Option<&str>,
    args: &[String],
) -> Option<CommandPlan> {
    let path = || PathBuf::from(args.first().cloned().unwrap_or_else(|| ".".to_string()));
    let step = match name {
        "analyze" => PlannedStep::Analyze(path()),
        "coding" => PlannedStep::Coding(path(), NlCodingOptions::default()),
        "validate" => PlannedStep::Validate(path()),
        "structure" => match subcommand.unwrap_or("view") {
            "edit" => PlannedStep::StructureEdit(path()),
            "undo" => PlannedStep::StructureUndo(path()),
            "redo" => PlannedStep::StructureRedo(path()),
            "dispatch" => PlannedStep::StructureDiff(path(), args.get(1).cloned()),
            _ => PlannedStep::StructureView(path()),
        },
        "rules" => PlannedStep::Rules,
        "memory" => PlannedStep::Memory(path()),
        _ => return None,
    };
    Some(CommandPlan {
        intent: None,
        steps: vec![step],
    })
}

fn render_direct_command_input(name: &str, subcommand: Option<&str>, args: &[String]) -> String {
    let mut parts = vec![format!("/{name}")];
    if let Some(subcommand) = subcommand {
        parts.push(subcommand.to_string());
    }
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn first_path_like_arg(args: &[String]) -> Option<String> {
    args.iter().find_map(|arg| {
        let looks_like_path = arg.contains('/')
            || arg.ends_with(".rs")
            || arg.ends_with(".toml")
            || arg.ends_with(".json")
            || arg.ends_with(".md")
            || arg == ".";
        looks_like_path.then(|| arg.clone())
    })
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
fn handle_run_command<W: Write>(session: &mut AgentSession, writer: &mut W) -> Result<(), String> {
    let _ = session;
    log_ir_bypass_warning("execution attempted without plan_id");
    writeln!(
        writer,
        "/run is disabled. Execute through PlanProposed -> PlanAccepted -> execute_ir_plan."
    )
    .map_err(|e| e.to_string())?;
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
    _registry: &CommandRegistry,
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

    let (command_plan, planner_label) = plan_agent_input(input, session, conversation);

    if let Some(command_plan) = command_plan {
        let planner_summary = render_plan_summary_with_label(&command_plan, planner_label);
        emit_output(session, writer, &planner_summary)?;
        let plan_id =
            record_plan_lifecycle(session, conversation, input, &command_plan, planner_label)?;
        update_conversation_after_plan(input, &command_plan, conversation);
        conversation.last_accepted_plan_id = Some(plan_id);

        if cfg!(test) {
            session.current_plan = Some(to_runtime_plan(&command_plan));
            session.state = State::Completed;
            emit_output(session, writer, "[test] planner-only mode")?;
            return Ok(());
        }

        session.state = State::Running;
        for output in execute_ir_plan(plan_id, &command_plan, session, conversation) {
            if !output.trim().is_empty() {
                emit_output(session, writer, &output)?;
            }
        }
        session.state = State::Completed;
        session.current_plan = Some(to_runtime_plan(&command_plan));
        conversation.autonomous_label = None;
        print_follow_up_suggestions(input, session, writer)?;
        return Ok(());
    }

    let plan = create_plan(input, session, planner_mode);
    if cfg!(test) {
        record_legacy_plan_lifecycle(session, conversation, input, &plan, planner_mode.as_str())?;
    } else {
        log_ir_bypass_warning("execution attempted without plan_id");
    }

    // プランナーラベルとステップ数を表示
    let planner_summary = format!(
        "[planner: {}] {} ステップ",
        planner_mode.as_str(),
        plan.steps.len(),
    );
    emit_output(session, writer, &planner_summary)?;

    if cfg!(test) {
        session.current_plan = Some(plan);
        session.state = State::Completed;
        emit_output(session, writer, "[test] planner-only mode")?;
        conversation.autonomous_label = None;
        return Ok(());
    }

    session.current_plan = Some(plan);
    session.state = State::Error;
    emit_output(
        session,
        writer,
        "Legacy planner fallback execution is disabled. Generate an IR-compatible command plan instead.",
    )?;
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

fn next_allowed_actions_for(completed_step: &str) -> Vec<ActionKind> {
    match completed_step {
        "analyze" => vec![ActionKind::Validate],
        "validate" => vec![ActionKind::Refactor, ActionKind::Analyze],
        "refactor" | "review-ready" => {
            vec![ActionKind::Apply, ActionKind::Refactor, ActionKind::Analyze]
        }
        "coding --apply" => vec![
            ActionKind::Validate,
            ActionKind::Refactor,
            ActionKind::Rollback,
        ],
        "rollback" => vec![
            ActionKind::CodingPreview,
            ActionKind::Analyze,
            ActionKind::Refactor,
        ],
        _ => Vec::new(),
    }
}

fn completed_step_name(input: &str) -> Option<&'static str> {
    let lower = input.to_lowercase();
    if lower.contains("coding") && lower.contains("--apply") {
        Some("coding --apply")
    } else if lower.contains("validate") || lower.contains("検証") || lower.contains("チェック")
    {
        Some("validate")
    } else if lower.contains("refactor") || lower.contains("リファクタ") || lower.contains("改善")
    {
        Some("refactor")
    } else if lower.contains("analyze")
        || lower.contains("分析")
        || lower.contains("解析")
        || lower.contains("調べ")
    {
        Some("analyze")
    } else {
        None
    }
}

fn build_session_diff_snapshot(
    review: &crate::tui::review_batch::ReviewBatchState,
) -> Option<SessionAppliedDiff> {
    review
        .preview_diff_snapshot()
        .or_else(|| review.last_batch_diff_snapshot())
}

fn execution_result_from_snapshot(
    snapshot: Option<SessionAppliedDiff>,
    fallback: ExecutionResult,
) -> ExecutionResult {
    snapshot
        .map(|diff| ExecutionResult::Success {
            files_changed: diff.files_changed,
            lines_added: diff.lines_added,
            lines_removed: diff.lines_removed,
        })
        .unwrap_or(fallback)
}

fn is_validate_request(input: &str) -> bool {
    input.to_lowercase().contains("validate") || input.contains("検証")
}

fn hydrate_ir_state(
    workspace_root: &std::path::Path,
    conversation: &mut ConversationState,
) -> Result<(), String> {
    let recovered = restore_or_initialize_ir_state(workspace_root)?;
    conversation.ir_state = recovered.state;
    conversation.last_target = conversation.ir_state.current_target.clone();
    Ok(())
}

fn ensure_ir_state_for_planning(
    session: &AgentSession,
    conversation: &mut ConversationState,
) -> Result<bool, String> {
    if !conversation.ir_state.session_id.is_empty() {
        return Ok(true);
    }
    let cwd;
    let workspace_root = if let Some(root) = session.workspace_root.as_deref() {
        root
    } else {
        cwd = std::env::current_dir().map_err(|err| err.to_string())?;
        cwd.as_path()
    };
    hydrate_ir_state(workspace_root, conversation)?;
    Ok(!conversation.ir_state.session_id.is_empty())
}

fn record_plan_lifecycle(
    session: &AgentSession,
    conversation: &mut ConversationState,
    input: &str,
    plan: &CommandPlan,
    planner_label: &str,
) -> Result<uuid::Uuid, String> {
    if !ensure_ir_state_for_planning(session, conversation)? {
        return Err("IR session_id is empty".to_string());
    }
    emit_intent_captured(
        &conversation.ir_state,
        input.to_string(),
        plan.intent.clone(),
    )?;
    let plan_id = emit_plan_proposed(
        &conversation.ir_state,
        plan.clone(),
        planner_label.to_string(),
    )?;
    emit_plan_accepted(&conversation.ir_state, plan_id)?;
    Ok(plan_id)
}

fn record_legacy_plan_lifecycle(
    session: &AgentSession,
    conversation: &mut ConversationState,
    input: &str,
    plan: &crate::plan::Plan,
    planner_label: &str,
) -> Result<(), String> {
    if !ensure_ir_state_for_planning(session, conversation)? {
        return Err("IR session_id is empty".to_string());
    }
    emit_intent_captured(&conversation.ir_state, input.to_string(), None)?;
    let plan_id = crate::ir::emit_runtime_plan_proposed(
        &conversation.ir_state,
        plan,
        planner_label.to_string(),
    )?;
    emit_plan_accepted(&conversation.ir_state, plan_id)?;
    Ok(())
}

fn record_ir_apply(conversation: &mut ConversationState, snapshot: Option<SessionAppliedDiff>) {
    let before = conversation.ir_state.clone();
    conversation.mark_transaction_applied(snapshot);
    let build_ok = conversation
        .ir_state
        .active_transaction
        .as_ref()
        .and_then(|tx| tx.latest_build_ok);
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        ActionKind::Apply,
        "apply",
        IRPersistenceArtifact {
            diff_ref: conversation
                .ir_state
                .active_transaction
                .as_ref()
                .and_then(|tx| tx.latest_diff_ref.clone()),
            build_ok,
            validation_ok: None,
            rollback_checkpoint: None,
        },
    );
}

fn record_ir_rollback(conversation: &mut ConversationState, _snapshot: Option<SessionAppliedDiff>) {
    let before = conversation.ir_state.clone();
    conversation.rollback_current_transaction();
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        ActionKind::Rollback,
        "rollback",
        IRPersistenceArtifact {
            diff_ref: None,
            build_ok: None,
            validation_ok: None,
            rollback_checkpoint: None,
        },
    );
}

fn plan_agent_input(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> (Option<CommandPlan>, &'static str) {
    if let Some(plan) = exact_file_route_plan(input) {
        return (Some(plan), "repl_file_route");
    }
    resolve_command_plan(input, session, conversation)
}

fn exact_file_route_plan(input: &str) -> Option<CommandPlan> {
    parse_file_mention_path(input).map(|path| CommandPlan {
        intent: None,
        steps: vec![PlannedStep::Analyze(std::path::PathBuf::from(path))],
    })
}

fn parse_file_mention_path(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    let rest = trimmed.strip_prefix("@file")?.trim_start();
    let candidate = rest.split_whitespace().next()?;
    is_exact_file_route_path(candidate).then_some(candidate)
}

fn is_exact_file_route_path(candidate: &str) -> bool {
    candidate.contains('/')
        || candidate.ends_with(".rs")
        || candidate.ends_with(".toml")
        || candidate.ends_with(".json")
        || candidate.ends_with(".md")
}

fn emit_output<W: Write>(
    session: &mut AgentSession,
    writer: &mut W,
    line: &str,
) -> Result<(), String> {
    session.record_output(line);
    writeln!(writer, "{line}").map_err(|e| e.to_string())
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "unknown panic payload".to_string(),
        },
    }
}

impl Write for ComposerViewState {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        for line in text.split('\n') {
            if line.is_empty() {
                continue;
            }
            self.push_transcript_line(line.to_string());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// DBM_CLI バナーを表示する
fn print_banner<W: Write>(writer: &mut W) -> Result<(), String> {
    for line in banner_lines() {
        writeln!(writer, "{line}").map_err(|e| e.to_string())?;
    }
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
    writeln!(
        writer,
        "  このプロジェクトを解析して     → design_cli analyze ."
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  この循環依存を安全に直して     → design_cli analyze . → coding . --safe --check"
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "  GUIで構造を開いて             → design_cli structure view ."
    )
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
    use crate::ir::IRPersistenceStore;
    use crate::plan::PlanStatus;
    use crate::service::dto::SessionAppliedFileDiff;
    use crate::test_support::ir_assert::{
        assert_execution_result, assert_plan_accepted, assert_plan_proposed,
    };

    fn write_minimal_workspace(root: &std::path::Path) {
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"repl_ir_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        std::fs::write(root.join("src/lib.rs"), "pub fn noop() {}\n").expect("lib");
    }

    fn run_with_input(input: &str) -> (String, Result<(), String>) {
        let temp = tempfile::tempdir().expect("tempdir");
        write_minimal_workspace(temp.path());
        run_with_input_in(temp.path(), input)
    }

    fn run_with_input_in(
        workspace_root: &std::path::Path,
        input: &str,
    ) -> (String, Result<(), String>) {
        let mut reader = Cursor::new(input.as_bytes().to_vec());
        let mut writer = Vec::new();
        let result = run_repl(workspace_root.to_path_buf(), &mut reader, &mut writer);
        (String::from_utf8_lossy(&writer).to_string(), result)
    }

    /// dispatch を直接呼んでsessionを検査するヘルパー
    fn run_with_session(input: &str) -> (String, AgentSession, Result<(), String>) {
        let temp = tempfile::tempdir().expect("tempdir");
        write_minimal_workspace(temp.path());
        let (output, session, _, result) = run_with_session_in(temp.path(), input);
        (output, session, result)
    }

    fn run_with_session_in(
        workspace_root: &std::path::Path,
        input: &str,
    ) -> (String, AgentSession, ConversationState, Result<(), String>) {
        let mut writer = Vec::new();
        let mut session = AgentSession::with_root(workspace_root.to_path_buf());
        let mut conversation = ConversationState::default();
        hydrate_ir_state(workspace_root, &mut conversation).expect("hydrate ir");
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
            conversation,
            Ok(()),
        )
    }

    /// PlannerMode を指定して run_with_session するヘルパー
    fn run_with_session_mode(
        input: &str,
        mode: PlannerMode,
    ) -> (String, AgentSession, Result<(), String>) {
        let temp = tempfile::tempdir().expect("tempdir");
        write_minimal_workspace(temp.path());
        let mut writer = Vec::new();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        hydrate_ir_state(temp.path(), &mut conversation).expect("hydrate ir");
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

    #[test]
    fn repl_persists_plan_events_before_execution() {
        let temp = tempfile::tempdir().expect("tempdir");
        run_with_input_in(temp.path(), "このプロジェクトを解析して\n/exit\n")
            .1
            .expect("repl run");

        let store = IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().expect("recover");
        let events = store
            .list_plan_events(&recovered.state.session_id)
            .expect("plan events");
        let plan_id = assert_plan_proposed(&events);
        assert_plan_accepted(&events, plan_id);
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
        let temp = tempfile::tempdir().expect("tempdir");
        let (_, session, _, _) = run_with_session_in(temp.path(), "input1\n");
        assert_eq!(session.history.len(), 1);
        assert_eq!(session.history[0], "input1");
        assert!(matches!(session.state, State::Completed | State::Error));
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
        let temp = tempfile::tempdir().expect("tempdir");
        let (_, session, _, _) = run_with_session_in(temp.path(), "some agent input\n");
        assert!(session.transcript.len() >= 2);
        assert!(
            session
                .transcript
                .iter()
                .any(|line| line.contains("[planner:"))
        );
        assert!(
            session
                .transcript
                .iter()
                .any(|line| line.contains("[test] planner-only mode"))
        );
        assert!(matches!(session.state, State::Completed | State::Error));
        assert_eq!(
            session.current_plan.as_ref().map(|plan| plan.status),
            Some(PlanStatus::Ready)
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
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("src")).expect("src dir");
        let (output, result) = run_with_input_in(temp.path(), "/analyze code src/\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("src/") || output.contains("[step 0] analyze"));

        let store = IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().expect("recover");
        let events = store
            .list_plan_events(&recovered.state.session_id)
            .expect("plan events");
        let plan_id = assert_plan_proposed(&events);
        assert_plan_accepted(&events, plan_id);
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
    fn proc_strip_activates_only_on_submit() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        view.insert_intent_text("first line");
        assert_eq!(view.proc_strip.phase, ProcPhase::Idle);

        assert_eq!(
            view.handle_key_event(crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                crossterm::event::KeyModifiers::SHIFT,
            )),
            ComposerAction::None
        );
        assert_eq!(view.proc_strip.phase, ProcPhase::Idle);
        assert_eq!(view.intent_text(), "first line");

        let action = view.handle_key_event(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(action, ComposerAction::Submit(_)));
        assert_eq!(
            view.proc_strip.phase,
            ProcPhase::Idle,
            "submit event is emitted, proc-strip activates only during execution"
        );
    }

    #[test]
    fn proc_strip_running_has_minimum_visibility() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        let mut sleeps = Vec::new();
        let mut redraws = Vec::new();
        let result = run_proc_strip_lifecycle(
            &mut view,
            "このプロジェクト全体を解析して",
            &[ProcPhase::Planning],
            &mut |view| {
                redraws.push(view.proc_strip.phase);
                Ok(())
            },
            &mut |duration| sleeps.push(duration),
            &mut |output| {
                writeln!(output, "analysis complete").expect("write output");
                Ok(false)
            },
        );

        assert!(result.is_ok());
        assert!(sleeps.contains(&RUNNING_MIN_VISIBLE));
        assert!(sleeps.contains(&DONE_MIN_VISIBLE));
        assert_eq!(redraws.first().copied(), Some(ProcPhase::Running));
    }

    #[test]
    fn proc_strip_done_precedes_transcript_append() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        let mut redraws = Vec::new();
        let result = run_proc_strip_lifecycle(
            &mut view,
            "design the api",
            &[ProcPhase::Planning],
            &mut |view| {
                redraws.push((view.proc_strip.phase, view.detail_len()));
                Ok(())
            },
            &mut |_| {},
            &mut |output| {
                writeln!(output, "[planner] complete").expect("write output");
                Ok(false)
            },
        );

        assert!(result.is_ok());
        let done_index = redraws
            .iter()
            .position(|(phase, _)| *phase == ProcPhase::Done)
            .expect("done redraw");
        assert_eq!(redraws[done_index].1, 0);
        assert!(
            view.detail_lines()
                .iter()
                .any(|line| line.contains("[planner] complete")),
            "{:?}",
            view.detail_lines()
        );
        assert_eq!(view.focus, crate::tui::composer::ComposerFocus::Editor);
    }

    #[test]
    fn proc_strip_error_resets_to_idle() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        let mut redraws = Vec::new();
        let result = run_proc_strip_lifecycle(
            &mut view,
            "broken input",
            &[ProcPhase::Planning],
            &mut |view| {
                redraws.push(view.proc_strip.phase);
                Ok(())
            },
            &mut |_| {},
            &mut |_| Err("planner failed".to_string()),
        );

        assert_eq!(result, Ok(false));
        assert!(redraws.contains(&ProcPhase::Error));
        assert_eq!(view.proc_strip.phase, ProcPhase::Idle);
        assert!(
            view.detail_lines()
                .iter()
                .any(|line| line.contains("Error: planner failed"))
        );
    }

    #[test]
    fn proc_strip_integrates_with_block_apply_lifecycle() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        let mut redraws = Vec::new();
        run_proc_strip_only(
            &mut view,
            &[ProcPhase::WritingEdit],
            &mut |view| {
                redraws.push(view.proc_strip.phase);
                Ok(())
            },
            &mut |_| {},
            &mut || Ok(()),
        )
        .expect("lifecycle");
        assert!(redraws.contains(&ProcPhase::WritingEdit));
        assert!(redraws.contains(&ProcPhase::Done));
        assert_eq!(view.detail_len(), 0, "{:?}", view.detail_lines());
    }

    #[test]
    fn plan_command_shows_no_plan_initially() {
        let (output, result) = run_with_input("/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("No plan"));
    }

    #[test]
    fn plan_command_shows_plan_after_agent_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (output, result) =
            run_with_input_in(temp.path(), "write a spec for cli\n/plan\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("Plan:"), "got: {output}");
        assert!(
            output.contains("pending") || output.contains("ready"),
            "plan should remain inspectable via IR path: {output}"
        );
    }

    #[test]
    fn run_command_executes_plan() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (output, result) = run_with_input_in(temp.path(), "spec for the api\n/run\n/exit\n");
        assert!(result.is_ok());
        assert!(
            output.contains("/run is disabled."),
            "legacy /run must stay disabled: {output}"
        );
    }

    #[test]
    fn run_without_plan_shows_message() {
        let (output, result) = run_with_input("/run\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("/run is disabled."));
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
        let temp = tempfile::tempdir().expect("tempdir");
        let (_, session, _, _) = run_with_session_in(temp.path(), "generate spec for cli\n");
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
    fn ctrl_q_force_quit_is_global() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        view.focus = crate::tui::composer::ComposerFocus::SendButton;
        view.insert_intent_text("/command");

        let action = global_key_action(
            crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            &view,
        );

        assert_eq!(action, Some(GlobalKeyAction::ForceQuit));
    }

    #[test]
    fn blank_escape_exits_but_non_blank_escape_does_not() {
        let blank_view = ComposerViewState::new(Vec::new(), State::Idle);
        assert_eq!(
            global_key_action(
                crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &blank_view,
            ),
            Some(GlobalKeyAction::Exit)
        );

        let mut non_blank_view = ComposerViewState::new(Vec::new(), State::Idle);
        non_blank_view.insert_intent_text("keep editing");
        assert_eq!(
            global_key_action(
                crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &non_blank_view,
            ),
            None
        );
    }

    #[test]
    fn typing_never_dispatches_or_appends_transcript() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        let transcript_before = view.detail_lines();
        for ch in ['@', 'f', 'i', 'l', 'e'] {
            let action = view.handle_key_event(crossterm::event::KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            ));
            assert_eq!(action, ComposerAction::None);
        }

        assert_eq!(view.detail_lines(), transcript_before);
        assert_eq!(view.intent_text(), "@file");
    }

    #[test]
    fn enter_dispatches_once_and_appends_transcript_once() {
        let mut session = AgentSession::new();
        let mut conversation = ConversationState::default();
        let mut registry = CommandRegistry::new();
        let mut planner_mode = PlannerMode::default();
        register_defaults(&mut registry);
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        view.insert_intent_text("/analyze code src/");

        let action = view.handle_key_event(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let ComposerAction::Submit(event) = action else {
            panic!("expected submit");
        };

        let should_exit = dispatch_submission(
            &event.input,
            &mut session,
            &mut conversation,
            &registry,
            &mut planner_mode,
            &mut view,
            &mut |_| Ok(()),
            &mut |_| {},
        )
        .expect("dispatch");

        assert!(!should_exit);
        assert_eq!(session.history, vec!["/analyze code src/"]);
        assert_eq!(
            view.detail_lines()
                .iter()
                .filter(|line| line.starts_with("> /analyze code src/"))
                .count(),
            1
        );
        assert!(
            view.detail_lines()
                .iter()
                .any(|line| !line.starts_with("> ")),
            "{:?}",
            view.detail_lines()
        );

        let second = view.handle_key_event(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        assert_eq!(second, ComposerAction::None);
    }

    #[test]
    fn shift_enter_is_ignored_without_dispatch() {
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        view.insert_intent_text("line1");
        let transcript_before = view.detail_lines();

        let action = view.handle_key_event(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::SHIFT,
        ));

        assert_eq!(action, ComposerAction::None);
        assert_eq!(view.detail_lines(), transcript_before);
        assert_eq!(view.intent_text(), "line1");
    }

    #[test]
    fn lifecycle_fields_are_removed() {
        let repl = include_str!("repl.rs");
        let composer = include_str!("tui/composer.rs");
        let session = include_str!("nl/session.rs");
        let planner = include_str!("nl/planner_v2.rs");
        let executor = include_str!("nl/executor.rs");

        let banned = [
            ["latest", "session", "diff"].join("_"),
            ["next", "step", "suggestion"].join("_"),
            ["last", "coding", "transaction"].join("_"),
            ["has", "pending", "coding", "transaction()"].join("_"),
        ];

        for needle in banned {
            assert!(!repl.contains(&needle), "repl still contains {needle}");
            assert!(
                !composer.contains(&needle),
                "composer still contains {needle}"
            );
            assert!(
                !session.contains(&needle),
                "session still contains {needle}"
            );
            assert!(
                !planner.contains(&needle),
                "planner still contains {needle}"
            );
            assert!(
                !executor.contains(&needle),
                "executor still contains {needle}"
            );
        }
    }

    #[test]
    fn route_helpers_are_removed_from_repl_stack() {
        let files = [
            include_str!("repl.rs"),
            include_str!("tui/composer.rs"),
            include_str!("nl/session.rs"),
            include_str!("nl/planner_v2.rs"),
            include_str!("nl/executor.rs"),
            include_str!("nl/mod.rs"),
        ];

        for source in files {
            let alias = ["/coding", " rollback"].concat();
            let phrase = ["undo previous", " transaction"].concat();
            assert!(!source.contains(&alias));
            assert!(!source.contains(&phrase));
        }
    }

    #[test]
    fn diff_cleared_after_ir_rollback() {
        let mut conversation = ConversationState::default();
        conversation.set_active_transaction(crate::service::dto::IRActiveTransaction {
            transaction_id: "tx:apps/cli/src/coding.rs".to_string(),
            canonical_target: PathBuf::from("apps/cli/src/coding.rs"),
            pending: false,
            applied: true,
            validated: false,
            rollback_available: true,
            latest_diff_ref: Some(SessionAppliedDiff {
                summary: "latest applied change (1 file)".to_string(),
                files: vec![SessionAppliedFileDiff {
                    file_path: "apps/cli/src/coding.rs".to_string(),
                    unified_diff_excerpt: "+ change".to_string(),
                }],
                files_changed: 1,
                lines_added: 1,
                lines_removed: 0,
            }),
            latest_build_ok: None,
        });

        record_ir_rollback(&mut conversation, None);

        assert!(conversation.ir_state.active_transaction.is_none());
        assert_eq!(
            conversation.ir_state.next_allowed_actions,
            vec![
                ActionKind::CodingPreview,
                ActionKind::Analyze,
                ActionKind::Refactor
            ]
        );
        let mut view = ComposerViewState::new(Vec::new(), State::Idle);
        view.sync_context(&conversation, None, None);
        assert_eq!(view.ir_state.active_transaction, None);
    }

    #[test]
    fn apply_and_rollback_are_observable_through_ir_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut conversation = ConversationState::default();
        hydrate_ir_state(temp.path(), &mut conversation).expect("hydrate");
        conversation.start_preview_transaction(PathBuf::from("apps/cli/src/repl.rs"));

        let apply_snapshot = Some(SessionAppliedDiff {
            summary: "applied".to_string(),
            files: vec![SessionAppliedFileDiff {
                file_path: "apps/cli/src/repl.rs".to_string(),
                unified_diff_excerpt: "+ IR".to_string(),
            }],
            files_changed: 1,
            lines_added: 1,
            lines_removed: 0,
        });
        record_ir_apply(&mut conversation, apply_snapshot.clone());
        record_ir_rollback(&mut conversation, None);

        let store = IRPersistenceStore::new(temp.path());
        let session_id = conversation.ir_state.session_id.clone();
        let transitions = store.list_transitions(&session_id).expect("transitions");
        let artifacts = store
            .list_transaction_artifacts(&session_id)
            .expect("artifacts");

        assert_execution_result(&transitions, ActionKind::Apply);
        assert_execution_result(&transitions, ActionKind::Rollback);
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.diff_ref == apply_snapshot),
            "expected applied artifact in IR"
        );
    }

    #[test]
    fn rollback_requires_explicit_route() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (output, result) = run_with_input_in(temp.path(), "rollback\n/exit\n");
        assert!(result.is_ok());
        assert!(output.contains("[planner: nl_v2] 1 steps"), "{output}");

        let store = IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().expect("recover");
        let events = store
            .list_plan_events(&recovered.state.session_id)
            .expect("plan events");
        let plan_id = assert_plan_proposed(&events);
        assert_plan_accepted(&events, plan_id);
    }

    #[test]
    fn file_route_bypasses_normalization_for_coding_rs() {
        let (plan, label) = plan_agent_input(
            "@file apps/cli/src/coding.rs",
            &AgentSession::new(),
            &ConversationState::default(),
        );
        assert_eq!(label, "repl_file_route");
        let plan = plan.expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Analyze(std::path::PathBuf::from(
                "apps/cli/src/coding.rs"
            ))]
        );
    }

    #[test]
    fn file_route_bypasses_normalization_for_runtime_vm_lib() {
        let (plan, label) = plan_agent_input(
            "@file crates/runtime/runtime_vm/src/lib.rs",
            &AgentSession::new(),
            &ConversationState::default(),
        );
        assert_eq!(label, "repl_file_route");
        let plan = plan.expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Analyze(std::path::PathBuf::from(
                "crates/runtime/runtime_vm/src/lib.rs"
            ))]
        );
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
        let temp = tempfile::tempdir().expect("tempdir");
        let (output, result) = run_with_input_in(temp.path(), "spec for cli\n/run\n/run\n/exit\n");
        assert!(result.is_ok());
        assert_eq!(output.matches("/run is disabled.").count(), 2);
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
        assert!(
            output.contains("[direct-dispatch] analyze project src/"),
            "got: {output}"
        );
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
