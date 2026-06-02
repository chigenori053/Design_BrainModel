use std::sync::{Mutex, OnceLock};

use crossterm::event::{KeyCode, KeyModifiers};

static RENDER_TRACE: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
static KEY_EVENT_TRACE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn trace_events() -> &'static Mutex<Vec<&'static str>> {
    RENDER_TRACE.get_or_init(|| Mutex::new(Vec::new()))
}

fn key_event_trace() -> &'static Mutex<Vec<String>> {
    KEY_EVENT_TRACE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn record(event: &'static str) {
    trace_events().lock().expect("render trace").push(event);
}

pub fn record_payload_dump(
    begin: &'static str,
    end: &'static str,
    line_prefix: &str,
    payload: &str,
) {
    record(begin);
    for line in payload.lines() {
        record(Box::leak(line.to_string().into_boxed_str()));
    }
    record(end);
    for (idx, line) in payload.lines().enumerate() {
        let event = if line.is_empty() {
            format!("{line_prefix}_LINE_{}", idx + 1)
        } else {
            format!("{line_prefix}_LINE_{} {line}", idx + 1)
        };
        record(Box::leak(event.into_boxed_str()));
    }
    if payload.is_empty() {
        record(Box::leak(format!("{line_prefix}_LINE_1").into_boxed_str()));
    }
}

pub fn record_key_event(code: KeyCode, modifiers: KeyModifiers) {
    key_event_trace()
        .lock()
        .expect("key event trace")
        .push(format!(
            "KEY={:?} MOD={:?} BITS={:#0x}",
            code,
            modifiers,
            modifiers.bits()
        ));
}

#[cfg(test)]
pub fn reset() {
    trace_events().lock().expect("render trace").clear();
    key_event_trace().lock().expect("key event trace").clear();
}

#[cfg(test)]
pub fn snapshot() -> Vec<&'static str> {
    trace_events().lock().expect("render trace").clone()
}

#[cfg(test)]
pub fn key_event_snapshot() -> Vec<String> {
    key_event_trace().lock().expect("key event trace").clone()
}
