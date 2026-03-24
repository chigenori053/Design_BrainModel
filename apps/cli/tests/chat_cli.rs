use std::io::Cursor;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use design_cli::app::build_runtime;
use design_cli::r#loop::{LoopSignal, step};
use design_cli::session::{ChatSession, merge_slots};
use runtime_core::intent_refiner::{Clarification, CoreSlot, SlotMap, SlotSource, SlotValue};
use runtime_core::{OptionalSlot, QualitySlot, SystemSlot};

#[test]
fn clarification_then_execution_works_with_slot_retention() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();

    let mut first = Cursor::new("build api\n");
    let signal = step(&runtime, &mut session, &mut first, &mut output).expect("first step");
    assert_eq!(signal, LoopSignal::Continue);
    assert_eq!(session.history, vec!["build api".to_string()]);
    assert!(session.pending_clarification.is_some());
    let pending_slots = session
        .slot_state
        .clone()
        .expect("partial slots should be stored");
    assert_eq!(
        pending_slots
            .core
            .get(&CoreSlot::InterfaceType)
            .unwrap()
            .value,
        "api"
    );
    let rendered = String::from_utf8(output.clone()).expect("utf8");
    assert!(rendered.contains("Which language and framework do you want?"));
    assert!(rendered.contains("Current:"));
    assert!(rendered.contains("Interface: api"));

    output.clear();
    let mut second = Cursor::new("rust\n");
    let signal = step(&runtime, &mut session, &mut second, &mut output).expect("second step");
    assert_eq!(signal, LoopSignal::Continue);
    assert_eq!(
        session.history,
        vec!["build api".to_string(), "rust".to_string()]
    );
    let slot_state = session
        .slot_state
        .clone()
        .expect("slot state should persist");
    assert!(session.pending_clarification.is_none());
    assert_eq!(
        slot_state.core.get(&CoreSlot::InterfaceType).unwrap().value,
        "api"
    );
    assert_eq!(
        slot_state.core.get(&CoreSlot::Language).unwrap().value,
        "rust"
    );
    assert_eq!(
        slot_state.core.get(&CoreSlot::Framework).unwrap().value,
        "axum"
    );
    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("✔ Project generated"));
    assert!(rendered.contains("[Intent]"));
    assert!(rendered.contains("Language: rust (explicitly specified)"));
    assert!(rendered.contains("Framework: axum (default applied)"));
    assert!(rendered.contains("Interface: api"));
}

#[test]
fn same_history_and_inputs_produce_same_output() {
    let run_once = || {
        let runtime = build_runtime();
        let mut session = ChatSession::new();
        let mut output = Vec::new();

        let mut first = Cursor::new("build api\n");
        step(&runtime, &mut session, &mut first, &mut output).expect("first step");

        let mut second = Cursor::new("rust\n");
        step(&runtime, &mut session, &mut second, &mut output).expect("second step");

        (
            session.history.clone(),
            session.slot_state.clone(),
            String::from_utf8(output).expect("utf8"),
        )
    };

    let lhs = run_once();
    let rhs = run_once();
    assert_eq!(lhs, rhs);
}

#[test]
fn reset_command_clears_session_state() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();

    let mut first = Cursor::new("build api\n");
    step(&runtime, &mut session, &mut first, &mut output).expect("first step");
    assert_eq!(session.history.len(), 1);

    let mut reset = Cursor::new("/reset\n");
    step(&runtime, &mut session, &mut reset, &mut output).expect("reset step");
    assert!(session.history.is_empty());
    assert!(session.slot_state.is_none());
    assert!(session.pending_clarification.is_none());
}

#[test]
fn exit_command_stops_loop_step() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();
    let mut exit = Cursor::new("/exit\n");

    let signal = step(&runtime, &mut session, &mut exit, &mut output).expect("exit step");
    assert_eq!(signal, LoopSignal::Exit);
}

#[test]
fn quit_command_stops_loop_step() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();
    let mut exit = Cursor::new("quit\n");

    let signal = step(&runtime, &mut session, &mut exit, &mut output).expect("quit step");
    assert_eq!(signal, LoopSignal::Exit);
}

#[test]
fn eof_stops_loop_step() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();
    let mut eof = Cursor::new("");

    let signal = step(&runtime, &mut session, &mut eof, &mut output).expect("eof step");
    assert_eq!(signal, LoopSignal::Exit);
}

#[test]
fn merge_slots_overwrites_with_new_values() {
    let mut prev = SlotMap::default();
    prev.core.insert(
        CoreSlot::Language,
        SlotValue::new("rust".to_string(), 1.0, SlotSource::Explicit),
    );
    prev.core.insert(
        CoreSlot::Framework,
        SlotValue::new("axum".to_string(), 1.0, SlotSource::Explicit),
    );
    prev.system.insert(
        SystemSlot::Runtime,
        SlotValue::new("tokio".to_string(), 1.0, SlotSource::Default),
    );
    prev.quality.insert(
        QualitySlot::Determinism,
        SlotValue::new("stable_v03".to_string(), 1.0, SlotSource::Default),
    );
    prev.optional.insert(
        OptionalSlot::Testing,
        SlotValue::new("enabled".to_string(), 1.0, SlotSource::Default),
    );

    let mut new = SlotMap::default();
    new.core.insert(
        CoreSlot::Language,
        SlotValue::new("python".to_string(), 1.0, SlotSource::Explicit),
    );

    let merged = merge_slots(&prev, &new);
    assert_eq!(
        merged.core.get(&CoreSlot::Language).unwrap().value,
        "python"
    );
    assert_eq!(merged.core.get(&CoreSlot::Framework).unwrap().value, "axum");
    assert_eq!(
        merged.system.get(&SystemSlot::Runtime).unwrap().value,
        "tokio"
    );
}

#[test]
fn multi_turn_updates_keep_previous_and_add_new_values() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();

    let mut first = Cursor::new("build api\n");
    step(&runtime, &mut session, &mut first, &mut output).expect("first step");
    output.clear();

    let mut second = Cursor::new("rust\n");
    step(&runtime, &mut session, &mut second, &mut output).expect("second step");
    output.clear();

    let mut third = Cursor::new("postgres\n");
    step(&runtime, &mut session, &mut third, &mut output).expect("third step");

    let slot_state = session.slot_state.expect("slot state");
    assert_eq!(
        slot_state.core.get(&CoreSlot::Language).unwrap().value,
        "rust"
    );
    assert_eq!(
        slot_state.core.get(&CoreSlot::InterfaceType).unwrap().value,
        "api"
    );
    assert_eq!(
        slot_state.system.get(&SystemSlot::Runtime).unwrap().value,
        "postgres"
    );
}

#[test]
fn clarification_state_can_be_set_and_resolved() {
    let mut session = ChatSession::new();
    session.update_clarification(Clarification {
        missing: vec![CoreSlot::Language],
        message: "Which language do you want?".to_string(),
    });
    assert!(session.pending_clarification.is_some());
    session.resolve_clarification();
    assert!(session.pending_clarification.is_none());
}

#[test]
fn slash_analyze_bypasses_clarification_flow() {
    let runtime = build_runtime();
    let mut session = ChatSession::new();
    let mut output = Vec::new();

    let mut first = Cursor::new("build api\n");
    step(&runtime, &mut session, &mut first, &mut output).expect("first step");
    assert!(session.pending_clarification.is_some());

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_chat_loop_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write manifest");
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("write source");

    output.clear();
    let mut analyze = Cursor::new(format!("/analyze {}\n", dir.display()));
    let signal = step(&runtime, &mut session, &mut analyze, &mut output).expect("analyze step");
    assert_eq!(signal, LoopSignal::Continue);
    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("Analysis"));
    assert!(rendered.contains(&dir.display().to_string()));
}
