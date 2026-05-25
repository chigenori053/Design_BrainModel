use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use memory_engine::InMemoryEngine;
use runtime_core::intent_refiner::{DefaultIntentRefiner, IntentRefiner};
use runtime_core::{CoreRuntime, RuntimeExecutionResult};

use crate::command::{Command, parse_command};
use crate::input::{InputState, read_input};
use crate::refactor::PatchScope;
use crate::renderer::{
    render_analysis_report, render_coding_report, render_design_report, render_question,
    render_result, render_validation_report,
};
use crate::service::{
    analyze_path, build_design_report, build_refactoring_report, build_validation_report,
};
use crate::session::{ChatSession, merge_slots};
use crate::state::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopSignal {
    Continue,
    Exit,
}

pub fn run_loop<R, W>(runtime: &CoreRuntime, reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let mut session = ChatSession::new();
    loop {
        let signal = step(runtime, &mut session, reader, writer)
            .map_err(|err| format!("cli loop failed: {err}"))?;
        if signal == LoopSignal::Exit {
            break;
        }
    }
    Ok(())
}

pub fn step<R, W>(
    runtime: &CoreRuntime,
    session: &mut ChatSession,
    reader: &mut R,
    writer: &mut W,
) -> io::Result<LoopSignal>
where
    R: BufRead,
    W: Write,
{
    let input = match read_input(reader, writer, State::Idle)? {
        InputState::Line(input) => input,
        InputState::Eof => return Ok(LoopSignal::Exit),
    };
    match parse_command(&input) {
        Command::Exit => return Ok(LoopSignal::Exit),
        Command::Reset => {
            session.reset();
            return Ok(LoopSignal::Continue);
        }
        Command::None => {}
    }

    if input.is_empty() {
        return Ok(LoopSignal::Continue);
    }

    if handle_slash_command(&input, writer)? {
        return Ok(LoopSignal::Continue);
    }

    let merged_slots = build_merged_slots(session, &input)
        .map_err(|err| io::Error::other(format!("slot extraction failed: {err}")))?;
    let context = runtime_core::ChatContext {
        history: session.history.clone(),
        last_slots: merged_slots.clone(),
    };
    let result = runtime
        .execute_from_text(&input, &context)
        .map_err(|err| io::Error::other(format!("runtime error: {err:?}")))?;
    match result {
        RuntimeExecutionResult::Executed(runtime_result) => {
            render_result(writer, &runtime_result)?;
            session.update_success(&input, &runtime_result);
        }
        RuntimeExecutionResult::Clarification(clarification) => {
            render_question(writer, &clarification, merged_slots.as_ref())?;
            session.update_pending(&input, merged_slots, clarification);
        }
    }
    Ok(LoopSignal::Continue)
}

fn handle_slash_command<W: Write>(input: &str, writer: &mut W) -> io::Result<bool> {
    if !input.starts_with('/') {
        return Ok(false);
    }

    let mut parts = input.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok(false);
    };

    match command {
        "/analyze" => {
            let path = parts.next().unwrap_or(".");
            let report = analyze_path(&PathBuf::from(path))
                .map_err(|err| io::Error::other(format!("analyze failed: {err}")))?;
            render_analysis_report(writer, &report)?;
            Ok(true)
        }
        "/refactoring" => {
            let path = parts.next().unwrap_or(".");
            let report = build_refactoring_report(
                &PathBuf::from(path),
                false,
                &crate::coding::CodingOptions {
                    apply: true,
                    check: true,
                    no_build: false,
                    backup: false,
                    format: false,
                    safe_mode: false,
                    auto_commit: false,
                    confirm_commit: false,
                    prompt_commit: false,
                    auto_push: false,
                    confirm_push: false,
                    auto_pr: false,
                    confirm_pr: false,
                    pr_base: "main".to_string(),
                    patch_scope: PatchScope::WorkspaceWide,
                    explicit_target: None,
                },
            )
            .map_err(|err| io::Error::other(format!("refactoring failed: {err}")))?;
            render_coding_report(writer, &report)?;
            Ok(true)
        }
        "/design" => {
            let path = parts.next().unwrap_or(".");
            let report = build_design_report(&PathBuf::from(path))
                .map_err(|err| io::Error::other(format!("design failed: {err}")))?;
            render_design_report(writer, &report)?;
            Ok(true)
        }
        "/validate" => {
            let path = parts.next().unwrap_or(".");
            let report = build_validation_report(&PathBuf::from(path))
                .map_err(|err| io::Error::other(format!("validate failed: {err}")))?;
            render_validation_report(writer, &report)?;
            Ok(true)
        }
        "/help" => {
            writeln!(
                writer,
                "Slash commands: /analyze [path], /refactoring [path], /design [path], /validate [path], /reset, /quit"
            )?;
            writer.flush()?;
            Ok(true)
        }
        _ => {
            writeln!(writer, "Unknown slash command: {command}")?;
            writer.flush()?;
            Ok(true)
        }
    }
}

fn build_merged_slots(
    session: &ChatSession,
    input: &str,
) -> Result<Option<runtime_core::SlotMap>, String> {
    let new_slots = extract_slots(input)?;
    match (&session.slot_state, is_slot_map_empty(&new_slots)) {
        (Some(prev), false) => Ok(Some(merge_slots(prev, &new_slots))),
        (Some(prev), true) => Ok(Some(prev.clone())),
        (None, false) => Ok(Some(new_slots)),
        (None, true) => Ok(None),
    }
}

fn extract_slots(input: &str) -> Result<runtime_core::SlotMap, String> {
    let refiner = DefaultIntentRefiner::new(Arc::new(InMemoryEngine::default()));
    let (_, trace) = refiner
        .refine_with_trace(input, &runtime_core::ChatContext::default())
        .map_err(|err| format!("{err:?}"))?;
    Ok(merge_slots(&trace.inferred, &trace.extracted))
}

fn is_slot_map_empty(slots: &runtime_core::SlotMap) -> bool {
    slots.core.is_empty()
        && slots.system.is_empty()
        && slots.quality.is_empty()
        && slots.optional.is_empty()
}
