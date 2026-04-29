use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use design_cli::control_event::{ControlEvent, ControlResponse};

use crate::agent_client::AgentClient;
use crate::event_stream::{AgentEvent, EventStream, ExecutionEvent};
use crate::prompt_builder::{PromptBuilder, RetryPromptContext};
use crate::response_mapper::{ResponseMapper, RetryError, RetryErrorKind};
use crate::session::Session;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Info,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentLoopConfig {
    pub log_level: LogLevel,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
        }
    }
}

pub trait ExecutorResponseSink {
    fn send_response(&mut self, response: &ControlResponse) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct JsonlResponseSink {
    path: PathBuf,
}

impl JsonlResponseSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl ExecutorResponseSink for JsonlResponseSink {
    fn send_response(&mut self, response: &ControlResponse) -> Result<(), String> {
        append_json_line(&self.path, response)
    }
}

pub struct AgentLoop<C, S> {
    client: C,
    sink: S,
    prompt_builder: PromptBuilder,
    response_mapper: ResponseMapper,
    run_log_path: PathBuf,
    config: AgentLoopConfig,
}

impl<C, S> AgentLoop<C, S>
where
    C: AgentClient,
    S: ExecutorResponseSink,
{
    pub fn new(client: C, sink: S, run_log_path: impl Into<PathBuf>) -> Self {
        Self {
            client,
            sink,
            prompt_builder: PromptBuilder,
            response_mapper: ResponseMapper::default(),
            run_log_path: run_log_path.into(),
            config: AgentLoopConfig::default(),
        }
    }

    pub fn with_parts(
        client: C,
        sink: S,
        prompt_builder: PromptBuilder,
        response_mapper: ResponseMapper,
        run_log_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            client,
            sink,
            prompt_builder,
            response_mapper,
            run_log_path: run_log_path.into(),
            config: AgentLoopConfig::default(),
        }
    }

    pub fn with_config(mut self, config: AgentLoopConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_log_level(&mut self, log_level: LogLevel) {
        self.config.log_level = log_level;
    }

    pub fn log_level(&self) -> LogLevel {
        self.config.log_level
    }

    fn should_log(&self, level: LogLevel) -> bool {
        level <= self.config.log_level
    }

    fn log_json<T: serde::Serialize>(&self, level: LogLevel, value: &T) -> Result<(), String> {
        if self.should_log(level) {
            append_json_line(&self.run_log_path, value)?;
        }
        Ok(())
    }

    pub fn run<R: BufRead>(
        &mut self,
        stream: &mut EventStream<R>,
        session: &mut Session,
    ) -> Result<LoopStatus, String> {
        while let Some(event) = stream.next_event()? {
            match event {
                AgentEvent::Control(control_event) => {
                    self.handle_control_event(&control_event, session)?;
                }
                AgentEvent::NonControl(non_control) => {
                    let status = match &non_control {
                        ExecutionEvent::RunCompleted(_) => Some(LoopStatus::Completed),
                        ExecutionEvent::RunFailed(_) => Some(LoopStatus::Failed),
                        ExecutionEvent::Other(_) => None,
                    };
                    session.record_event_value(non_control.into_value());
                    if let Some(status) = status {
                        return Ok(status);
                    }
                }
            }
        }
        Ok(LoopStatus::Running)
    }

    fn handle_control_event(
        &mut self,
        event: &ControlEvent,
        session: &mut Session,
    ) -> Result<(), String> {
        session.record_control_event(event)?;
        let mut last_raw = None::<String>;
        let mut last_error = None::<RetryError>;
        let max_retries = self.response_mapper.max_retries();

        for attempt in 0..max_retries {
            let prompt = match &last_error {
                Some(error) => self.prompt_builder.build_retry(
                    event,
                    session,
                    &RetryPromptContext {
                        retry_count: attempt.saturating_add(1),
                        max_retries,
                        last_error: error.kind.retry_reason().to_string(),
                    },
                )?,
                None => self.prompt_builder.build(event, session)?,
            };
            self.log_agent_prompt(event, attempt, &prompt)?;

            match self.client.call(prompt.clone()) {
                Ok(raw) => {
                    self.log_agent_response_raw(event, attempt, &raw)?;
                    last_raw = Some(raw.clone());
                    match self.response_mapper.parse(&raw, event) {
                        Ok(response) => {
                            self.log_agent_response_parsed(&response)?;
                            self.sink.send_response(&response)?;
                            return Ok(());
                        }
                        Err(err) if attempt.saturating_add(1) < max_retries => {
                            self.emit_retry_notice(&err, attempt.saturating_add(2), max_retries);
                            self.log_retry_attempt(event, attempt, &err)?;
                            last_error = Some(err);
                        }
                        Err(err) => {
                            self.log_retry_attempt(event, attempt, &err)?;
                            last_error = Some(err);
                            break;
                        }
                    }
                }
                Err(err) if attempt.saturating_add(1) < max_retries => {
                    let err = RetryError::new(RetryErrorKind::Agent, err);
                    self.emit_retry_notice(&err, attempt.saturating_add(2), max_retries);
                    self.log_retry_attempt(event, attempt, &err)?;
                    last_error = Some(err);
                }
                Err(err) => {
                    let err = RetryError::new(RetryErrorKind::Agent, err);
                    self.log_retry_attempt(event, attempt, &err)?;
                    last_error = Some(err);
                    break;
                }
            }
        }

        let response = self.response_mapper.default_response(event);
        self.emit_fallback_notice();
        self.log_fallback_triggered(event, last_raw.as_deref(), last_error.as_ref(), &response)?;
        self.sink.send_response(&response)
    }

    fn emit_retry_notice(&self, error: &RetryError, next_attempt: u8, max_attempts: u8) {
        eprintln!("→ {}", error.kind.label());
        eprintln!(
            "→ Retrying ({next_attempt}/{max_attempts}): {}",
            error.kind.retry_fix_hint()
        );
    }

    fn emit_fallback_notice(&self) {
        eprintln!("→ Fallback triggered");
    }

    fn log_agent_prompt(
        &self,
        event: &ControlEvent,
        attempt: u8,
        prompt: &str,
    ) -> Result<(), String> {
        self.log_json(
            LogLevel::Debug,
            &serde_json::json!({
                "type": "agent_prompt",
                "run_id": event.run_id,
                "step_id": event.step_id,
                "request_id": event.request_id,
                "attempt": attempt,
                "prompt": prompt,
            }),
        )
    }

    fn log_agent_response_raw(
        &self,
        event: &ControlEvent,
        attempt: u8,
        raw: &str,
    ) -> Result<(), String> {
        self.log_json(
            LogLevel::Debug,
            &serde_json::json!({
                "type": "agent_response_raw",
                "run_id": event.run_id,
                "step_id": event.step_id,
                "request_id": event.request_id,
                "attempt": attempt,
                "raw": raw,
            }),
        )
    }

    fn log_agent_response_parsed(&self, response: &ControlResponse) -> Result<(), String> {
        self.log_json(
            LogLevel::Info,
            &serde_json::json!({
                "type": "agent_response_parsed",
                "run_id": response.run_id,
                "step_id": response.step_id,
                "request_id": response.request_id,
                "response": response,
            }),
        )
    }

    fn log_retry_attempt(
        &self,
        event: &ControlEvent,
        attempt: u8,
        error: &RetryError,
    ) -> Result<(), String> {
        self.log_json(
            LogLevel::Info,
            &serde_json::json!({
                "type": "retry_attempt",
                "run_id": event.run_id,
                "step_id": event.step_id,
                "request_id": event.request_id,
                "attempt": attempt,
                "error_kind": error.kind.as_str(),
                "error": error.message,
                "reason": error.kind.retry_reason(),
            }),
        )?;
        self.log_json(
            LogLevel::Info,
            &serde_json::json!({
                "type": "retry_reason",
                "event": "retry_reason",
                "run_id": event.run_id,
                "step_id": event.step_id,
                "request_id": event.request_id,
                "attempt": attempt.saturating_add(1),
                "reason": error.kind.retry_reason(),
            }),
        )
    }

    fn log_fallback_triggered(
        &self,
        event: &ControlEvent,
        last_raw: Option<&str>,
        last_error: Option<&RetryError>,
        response: &ControlResponse,
    ) -> Result<(), String> {
        self.log_json(
            LogLevel::Info,
            &serde_json::json!({
                "type": "fallback_triggered",
                "run_id": event.run_id,
                "step_id": event.step_id,
                "request_id": event.request_id,
                "last_raw": last_raw,
                "last_error_kind": last_error.map(|err| err.kind.as_str()),
                "last_error": last_error.map(|err| err.message.as_str()),
                "last_reason": last_error.map(|err| err.kind.retry_reason()),
                "response": response,
            }),
        )
    }
}

impl ExecutionEvent {
    fn into_value(self) -> serde_json::Value {
        match self {
            Self::RunCompleted(value) | Self::RunFailed(value) | Self::Other(value) => value,
        }
    }
}

fn append_json_line<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("open {}: {err}", path.display()))?;
    let line = serde_json::to_string(value).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())
}

#[derive(Default)]
pub struct VecResponseSink {
    pub responses: Vec<ControlResponse>,
}

impl ExecutorResponseSink for VecResponseSink {
    fn send_response(&mut self, response: &ControlResponse) -> Result<(), String> {
        self.responses.push(response.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_client::MockAgentClient;
    use design_cli::control_event::{ControlEvent, DecisionAction, DecisionReason, RequestId};
    use std::io::Cursor;

    fn event() -> ControlEvent {
        ControlEvent::decision_required(
            "run",
            "step",
            RequestId::from_u128(42),
            DecisionReason::Ambiguity.as_str(),
            serde_json::json!({}),
            vec![DecisionAction::Retry, DecisionAction::Abort],
            DecisionAction::Abort,
        )
    }

    #[test]
    fn loop_maps_control_event_to_response() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let raw = serde_json::json!({
            "response_to": "decision_required",
            "request_id": event.request_id,
            "action": "retry"
        })
        .to_string();
        let client = MockAgentClient::from_json_responses(vec![raw]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        let status = loop_core.run(&mut stream, &mut session).unwrap();
        assert_eq!(status, LoopStatus::Running);
        assert_eq!(loop_core.sink.responses.len(), 1);
        assert_eq!(
            loop_core.sink.responses[0].action,
            Some(DecisionAction::Retry)
        );
    }

    #[test]
    fn invalid_agent_output_retries_then_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let client = MockAgentClient::from_json_responses(vec![
            "not-json".to_string(),
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": RequestId::from_u128(7),
                "action": "retry"
            })
            .to_string(),
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "skip"
            })
            .to_string(),
        ]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        loop_core.run(&mut stream, &mut session).unwrap();
        assert_eq!(loop_core.sink.responses.len(), 1);
        assert_eq!(
            loop_core.sink.responses[0].action,
            Some(DecisionAction::Abort)
        );
    }

    #[test]
    fn debug_logging_records_raw_response_and_retry_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let client = MockAgentClient::from_json_responses(vec![
            "not-json".to_string(),
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "retry"
            })
            .to_string(),
        ]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        loop_core.set_log_level(LogLevel::Debug);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        loop_core.run(&mut stream, &mut session).unwrap();
        let log = std::fs::read_to_string(log).unwrap();

        assert!(log.contains("\"type\":\"agent_prompt\""));
        assert!(log.contains("\"type\":\"agent_response_raw\""));
        assert!(log.contains("\"type\":\"retry_attempt\""));
        assert!(log.contains("\"type\":\"retry_reason\""));
        assert!(log.contains("\"type\":\"agent_response_parsed\""));
        assert!(log.contains("Previous attempt failed."));
        assert!(log.contains("Invalid JSON format"));
        assert!(log.contains("Retry attempt: 2/2"));
        assert!(log.contains("parse_error"));
    }

    #[test]
    fn schema_violation_retries_with_reason_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let client = MockAgentClient::from_json_responses(vec![
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "retry",
                "extra": true
            })
            .to_string(),
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "retry"
            })
            .to_string(),
        ]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        loop_core.set_log_level(LogLevel::Debug);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        loop_core.run(&mut stream, &mut session).unwrap();
        assert_eq!(loop_core.sink.responses.len(), 1);
        assert_eq!(
            loop_core.sink.responses[0].action,
            Some(DecisionAction::Retry)
        );
        let log = std::fs::read_to_string(log).unwrap();
        assert!(log.contains("Schema validation failed"));
    }

    #[test]
    fn action_violation_retries_with_reason_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let client = MockAgentClient::from_json_responses(vec![
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "skip"
            })
            .to_string(),
            serde_json::json!({
                "response_to": "decision_required",
                "request_id": event.request_id,
                "action": "retry"
            })
            .to_string(),
        ]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        loop_core.set_log_level(LogLevel::Debug);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        loop_core.run(&mut stream, &mut session).unwrap();
        assert_eq!(loop_core.sink.responses.len(), 1);
        assert_eq!(
            loop_core.sink.responses[0].action,
            Some(DecisionAction::Retry)
        );
        let log = std::fs::read_to_string(log).unwrap();
        assert!(log.contains("Invalid action"));
    }

    #[test]
    fn info_logging_omits_detailed_prompt_and_raw_response() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("run.jsonl");
        let event = event();
        let raw = serde_json::json!({
            "response_to": "decision_required",
            "request_id": event.request_id,
            "action": "retry"
        })
        .to_string();
        let client = MockAgentClient::from_json_responses(vec![raw]);
        let sink = VecResponseSink::default();
        let mut loop_core = AgentLoop::new(client, sink, &log);
        let line = serde_json::json!({"type": "event", "event": event}).to_string();
        let mut stream = EventStream::new(Cursor::new(format!("{line}\n")));
        let mut session = Session::new("run", "fix it");

        loop_core.run(&mut stream, &mut session).unwrap();
        let log = std::fs::read_to_string(log).unwrap();

        assert!(!log.contains("\"type\":\"agent_prompt\""));
        assert!(!log.contains("\"type\":\"agent_response_raw\""));
        assert!(log.contains("\"type\":\"agent_response_parsed\""));
    }
}
