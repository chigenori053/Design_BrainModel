use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use std::path::Path;

use design_cli::control_event::ControlEvent;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Control(ControlEvent),
    NonControl(ExecutionEvent),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionEvent {
    RunCompleted(serde_json::Value),
    RunFailed(serde_json::Value),
    Other(serde_json::Value),
}

pub struct EventStream<R: BufRead> {
    lines: Lines<R>,
}

impl EventStream<BufReader<File>> {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|err| format!("open event stream: {err}"))?;
        Ok(Self::new(BufReader::new(file)))
    }
}

impl<R: BufRead> EventStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            lines: reader.lines(),
        }
    }

    pub fn next_event(&mut self) -> Result<Option<AgentEvent>, String> {
        for line in self.lines.by_ref() {
            let line = line.map_err(|err| format!("read event stream: {err}"))?;
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value =
                serde_json::from_str(&line).map_err(|err| format!("parse event stream: {err}"))?;
            return Ok(Some(classify_event(value)?));
        }
        Ok(None)
    }
}

fn classify_event(value: serde_json::Value) -> Result<AgentEvent, String> {
    let event_value = value
        .get("event")
        .cloned()
        .filter(|event| event.is_object())
        .unwrap_or_else(|| value.clone());

    if let Ok(event) = serde_json::from_value::<ControlEvent>(event_value) {
        return Ok(AgentEvent::Control(event));
    }

    match serde_json::from_value::<ExecutionEventName>(value.clone())
        .ok()
        .and_then(|event| event.event)
    {
        Some(TerminalEventKind::RunCompleted) => {
            Ok(AgentEvent::NonControl(ExecutionEvent::RunCompleted(value)))
        }
        Some(TerminalEventKind::RunFailed) => {
            Ok(AgentEvent::NonControl(ExecutionEvent::RunFailed(value)))
        }
        None => Ok(AgentEvent::NonControl(ExecutionEvent::Other(value))),
    }
}

#[derive(Debug, Deserialize)]
struct ExecutionEventName {
    #[serde(default)]
    event: Option<TerminalEventKind>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalEventKind {
    RunCompleted,
    RunFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_cli::control_event::{DecisionAction, DecisionReason, RequestId};
    use std::io::Cursor;

    #[test]
    fn extracts_control_events_from_run_log_event_entry() {
        let event = ControlEvent::decision_required(
            "run",
            "step",
            RequestId::from_u128(1),
            DecisionReason::Ambiguity.as_str(),
            serde_json::json!({}),
            vec![DecisionAction::Abort],
            DecisionAction::Abort,
        );
        let line = serde_json::json!({
            "type": "event",
            "event": event,
            "timestamp": "2026-04-29T00:00:00Z"
        })
        .to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let next = stream.next_event().unwrap().unwrap();
        assert!(matches!(next, AgentEvent::Control(_)));
    }

    #[test]
    fn classifies_terminal_events_as_non_control() {
        let line = serde_json::json!({
            "event": "run_failed",
            "run_id": "run",
            "reason": "limit"
        })
        .to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let next = stream.next_event().unwrap().unwrap();

        assert!(matches!(
            next,
            AgentEvent::NonControl(ExecutionEvent::RunFailed(_))
        ));
    }
}
