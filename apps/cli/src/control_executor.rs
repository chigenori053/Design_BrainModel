//! DBM-CLI-CONTROL-EVENT-SPEC v1.1 — Executor + Run Logger
//!
//! Implements the blocking executor that emits Control Events and waits for
//! Agent responses (§8), with timeout handling (§9), response validation (§10),
//! and JSONL run logging (§11).
//!
//! # Implementations
//! - [`StdioControlExecutor`] — interactive CLI mode (reads stdin, writes stderr)
//! - [`ReplayControlExecutor`] — deterministic replay from a saved run log
//!
//! # Run log format (`.dbm/runs/<run_id>.jsonl`)
//! Each line is a JSON object with a `type` discriminant:
//! - `"event"`    — the emitted [`ControlEvent`] verbatim
//! - `"response"` — the raw [`ControlResponse`] from the agent
//! - `"decision"` — the resolved [`ControlOutcome`] (canonical record)

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::control_event::{
    ControlError, ControlEvent, ControlOutcome, ControlPayload, ControlResponse, DecisionAction,
    DecisionSource, RequestId, timestamp_now,
};

// ── Config ─────────────────────────────────────────────────────────────────────

/// Default response timeout in milliseconds (§9.1).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_MAX_ATTEMPTS: u8 = 3;
pub const DEFAULT_MAX_LOOPS: u8 = 5;
pub const MAX_ATTEMPTS_LIMIT: u8 = 10;
pub const MAX_LOOPS_LIMIT: u8 = 20;
pub const MIN_TIMEOUT_MS: u64 = 1_000;

/// Safe default action when no executor is provided (abort is the safest
/// no-op for most decision branches).
pub const DEFAULT_FALLBACK_ACTION: &str = "abort";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyConfig {
    pub max_attempts: u8,
    pub max_loops: u8,
    pub decision_timeout_ms: u64,
}

impl SafetyConfig {
    pub fn new(
        max_attempts: u8,
        max_loops: u8,
        decision_timeout_ms: u64,
    ) -> Result<Self, ControlError> {
        if max_attempts == 0 || max_attempts > MAX_ATTEMPTS_LIMIT {
            return Err(ControlError::InvalidState(format!(
                "max_attempts must be 1..={MAX_ATTEMPTS_LIMIT}, got {max_attempts}"
            )));
        }
        if max_loops == 0 || max_loops > MAX_LOOPS_LIMIT {
            return Err(ControlError::InvalidState(format!(
                "max_loops must be 1..={MAX_LOOPS_LIMIT}, got {max_loops}"
            )));
        }
        if decision_timeout_ms < MIN_TIMEOUT_MS {
            return Err(ControlError::InvalidState(format!(
                "decision_timeout_ms must be >= {MIN_TIMEOUT_MS}, got {decision_timeout_ms}"
            )));
        }
        Ok(Self {
            max_attempts,
            max_loops,
            decision_timeout_ms,
        })
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            max_loops: DEFAULT_MAX_LOOPS,
            decision_timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptCounter {
    pub step_id: String,
    pub attempts: u8,
    pub max_attempts: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionLoopGuard {
    pub run_id: String,
    pub loop_count: u8,
    pub max_loops: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafetyLimitType {
    MaxAttempts,
    MaxLoops,
    InvalidState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyLimitTriggered {
    pub event: String,
    pub limit_type: SafetyLimitType,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_count: Option<u8>,
    pub abort_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetySnapshot {
    pub event: String,
    pub run_id: String,
    pub step_id: String,
    pub attempts: u8,
    pub max_attempts: u8,
    pub loop_count: u8,
    pub max_loops: u8,
}

#[derive(Debug, Clone)]
pub struct ControlSafetyLayer {
    config: SafetyConfig,
    attempts: BTreeMap<String, AttemptCounter>,
    loop_guard: Option<DecisionLoopGuard>,
}

impl ControlSafetyLayer {
    pub fn new(config: SafetyConfig) -> Self {
        Self {
            config,
            attempts: BTreeMap::new(),
            loop_guard: None,
        }
    }

    pub fn config(&self) -> SafetyConfig {
        self.config
    }

    pub fn record_event(
        &mut self,
        event: &ControlEvent,
    ) -> Result<SafetySnapshot, SafetyLimitTriggered> {
        let guard = self.loop_guard.get_or_insert_with(|| DecisionLoopGuard {
            run_id: event.run_id.clone(),
            loop_count: 0,
            max_loops: self.config.max_loops,
        });
        if guard.run_id != event.run_id {
            return Err(SafetyLimitTriggered {
                event: "safety_limit_triggered".to_string(),
                limit_type: SafetyLimitType::InvalidState,
                run_id: event.run_id.clone(),
                step_id: Some(event.step_id.clone()),
                attempts: None,
                loop_count: Some(guard.loop_count),
                abort_reason: format!(
                    "safety guard run_id mismatch: expected {}, got {}",
                    guard.run_id, event.run_id
                ),
            });
        }
        guard.loop_count = guard.loop_count.saturating_add(1);
        let loop_count = guard.loop_count;
        let max_loops = guard.max_loops;

        let counter = self
            .attempts
            .entry(event.step_id.clone())
            .or_insert_with(|| AttemptCounter {
                step_id: event.step_id.clone(),
                attempts: 0,
                max_attempts: self.config.max_attempts,
            });
        counter.attempts = counter.attempts.saturating_add(1);
        let attempts = counter.attempts;
        let max_attempts = counter.max_attempts;

        if attempts > max_attempts {
            return Err(SafetyLimitTriggered {
                event: "safety_limit_triggered".to_string(),
                limit_type: SafetyLimitType::MaxAttempts,
                run_id: event.run_id.clone(),
                step_id: Some(event.step_id.clone()),
                attempts: Some(attempts),
                loop_count: Some(loop_count),
                abort_reason: format!(
                    "step {} exceeded max_attempts {}",
                    event.step_id, max_attempts
                ),
            });
        }
        if loop_count > max_loops {
            return Err(SafetyLimitTriggered {
                event: "safety_limit_triggered".to_string(),
                limit_type: SafetyLimitType::MaxLoops,
                run_id: event.run_id.clone(),
                step_id: Some(event.step_id.clone()),
                attempts: Some(attempts),
                loop_count: Some(loop_count),
                abort_reason: format!("run {} exceeded max_loops {}", event.run_id, max_loops),
            });
        }

        Ok(SafetySnapshot {
            event: "safety_snapshot".to_string(),
            run_id: event.run_id.clone(),
            step_id: event.step_id.clone(),
            attempts,
            max_attempts,
            loop_count,
            max_loops,
        })
    }
}

impl Default for ControlSafetyLayer {
    fn default() -> Self {
        Self::new(SafetyConfig::default())
    }
}

// ── RunLogEntry ───────────────────────────────────────────────────────────────

/// A single line in `.dbm/runs/<run_id>.jsonl` (§11).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunLogEntry {
    /// The emitted ControlEvent (verbatim JSON).
    Event {
        event: serde_json::Value,
        timestamp: String,
    },
    /// The raw ControlResponse received from the agent.
    Response {
        response: serde_json::Value,
        timestamp: String,
    },
    /// The resolved decision — canonical record for replay (§11).
    Decision {
        request_id: RequestId,
        step_id: String,
        outcome: ControlOutcome,
        timestamp: String,
    },
    SafetyLimitTriggered {
        #[serde(flatten)]
        safety: SafetyLimitTriggered,
        timestamp: String,
    },
    SafetySnapshot {
        #[serde(flatten)]
        safety: SafetySnapshot,
        timestamp: String,
    },
    RunFailed {
        event: String,
        run_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step_id: Option<String>,
        reason: String,
        timestamp: String,
    },
    AgentPrompt {
        run_id: String,
        step_id: String,
        request_id: RequestId,
        #[serde(default)]
        attempt: u8,
        prompt: String,
    },
    AgentResponseRaw {
        run_id: String,
        step_id: String,
        request_id: RequestId,
        attempt: u8,
        raw: String,
    },
    AgentResponseParsed {
        run_id: String,
        step_id: String,
        request_id: RequestId,
        response: ControlResponse,
    },
    RetryAttempt {
        run_id: String,
        step_id: String,
        request_id: RequestId,
        attempt: u8,
        error_kind: String,
        error: String,
    },
    FallbackTriggered {
        run_id: String,
        step_id: String,
        request_id: RequestId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_raw: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
        response: ControlResponse,
    },
}

// ── RunLogger ─────────────────────────────────────────────────────────────────

/// Appends [`RunLogEntry`] lines to `.dbm/runs/<run_id>.jsonl` (§11).
///
/// The file is created on first write; subsequent calls append.
pub struct RunLogger {
    path: PathBuf,
}

impl RunLogger {
    /// Create (or reuse) the logger for the given run.
    ///
    /// Creates `.dbm/runs/` if it does not already exist.
    pub fn new(workspace_root: &Path, run_id: &str) -> Result<Self, ControlError> {
        let dir = workspace_root.join(".dbm").join("runs");
        std::fs::create_dir_all(&dir)
            .map_err(|e| ControlError::LogError(format!("create runs dir: {e}")))?;
        Ok(Self {
            path: dir.join(format!("{run_id}.jsonl")),
        })
    }

    /// Return the path of the run log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one JSON line.
    pub fn append(&self, entry: &RunLogEntry) -> Result<(), ControlError> {
        use std::fs::OpenOptions;
        let line = serde_json::to_string(entry)
            .map_err(|e| ControlError::LogError(format!("serialize entry: {e}")))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| {
                ControlError::LogError(format!("open log {}: {e}", self.path.display()))
            })?;
        writeln!(file, "{line}").map_err(|e| ControlError::LogError(format!("write log: {e}")))?;
        Ok(())
    }

    /// Read all entries (used by [`ReplayControlExecutor`]).
    pub fn read_all(&self) -> Result<Vec<RunLogEntry>, ControlError> {
        let file = std::fs::File::open(&self.path)
            .map_err(|e| ControlError::IoError(format!("open log: {e}")))?;
        let mut entries = Vec::new();
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|e| ControlError::IoError(format!("read line: {e}")))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: RunLogEntry = serde_json::from_str(&line)
                .map_err(|e| ControlError::ParseError(format!("log line: {e}")))?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

// ── ControlExecutor trait ─────────────────────────────────────────────────────

/// The Executor calls [`emit`] and then blocks until the Agent responds.
///
/// The returned [`ControlOutcome`] drives the state-machine transition (§7):
/// - `Decision { action: "retry" }` → retry the step
/// - `Decision { action: "skip" }`  → mark step skipped, continue plan
/// - `Decision { action: "abort" }` → abort the run
/// - `Decision { action: "modify" }` → modify/proceed where allowed by the event
/// - `Input { data }` → feed `data` back into the step
pub trait ControlExecutor {
    /// Emit a Control Event and block until a response or timeout (§8.1–8.2).
    fn emit(&mut self, event: ControlEvent) -> Result<ControlOutcome, ControlError>;
}

// ── StdioControlExecutor ──────────────────────────────────────────────────────

/// Interactive CLI executor (§12).
///
/// - Prints a human-readable prompt to **stderr** so stdout stays machine-readable.
/// - Accepts JSON [`ControlResponse`] on **stdin**; plain text is debug-only.
/// - Times out after `timeout_ms` milliseconds and falls back to the `default` action.
/// - Writes event / response / decision records to the run log if a [`RunLogger`] is set.
pub struct StdioControlExecutor {
    logger: Option<RunLogger>,
    debug_plain_text: bool,
    safety: ControlSafetyLayer,
}

impl StdioControlExecutor {
    pub fn new(timeout_ms: u64, logger: Option<RunLogger>) -> Self {
        let config = SafetyConfig::new(
            DEFAULT_MAX_ATTEMPTS,
            DEFAULT_MAX_LOOPS,
            timeout_ms.max(MIN_TIMEOUT_MS),
        )
        .unwrap_or_default();
        Self {
            logger,
            debug_plain_text: false,
            safety: ControlSafetyLayer::new(config),
        }
    }

    pub fn with_safety_config(config: SafetyConfig, logger: Option<RunLogger>) -> Self {
        Self {
            logger,
            debug_plain_text: false,
            safety: ControlSafetyLayer::new(config),
        }
    }

    pub fn with_debug_plain_text(mut self) -> Self {
        self.debug_plain_text = true;
        self
    }

    /// Convenience constructor that uses the default timeout and creates a
    /// run log at `.dbm/runs/<run_id>.jsonl`.
    pub fn with_defaults(workspace_root: &Path, run_id: &str) -> Self {
        let logger = RunLogger::new(workspace_root, run_id).ok();
        Self::new(DEFAULT_TIMEOUT_MS, logger)
    }
}

impl ControlExecutor for StdioControlExecutor {
    fn emit(&mut self, event: ControlEvent) -> Result<ControlOutcome, ControlError> {
        // ── 1. Log the emitted event ──────────────────────────────────────────
        let event_json = serde_json::to_value(&event)
            .map_err(|e| ControlError::IoError(format!("serialize event: {e}")))?;
        if let Some(logger) = &self.logger {
            logger.append(&RunLogEntry::Event {
                event: event_json.clone(),
                timestamp: timestamp_now(),
            })?;
        }
        match self.safety.record_event(&event) {
            Ok(snapshot) => {
                if let Some(logger) = &self.logger {
                    logger.append(&RunLogEntry::SafetySnapshot {
                        safety: snapshot,
                        timestamp: timestamp_now(),
                    })?;
                }
            }
            Err(limit) => {
                return self.abort_for_safety_limit(&event, limit);
            }
        }

        // ── 2. Display human-readable prompt ──────────────────────────────────
        print_control_event_prompt(&event);

        // ── 3. Read response from stdin with timeout (§8.1, §9.1) ────────────
        let timeout = Duration::from_millis(self.safety.config().decision_timeout_ms);
        let (tx, rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_ok() {
                let _ = tx.send(line.trim().to_string());
            }
        });

        let (raw, timed_out) = match rx.recv_timeout(timeout) {
            Ok(line) => (line, false),
            Err(_) => (String::new(), true),
        };

        // ── 4. Resolve the outcome ────────────────────────────────────────────
        let outcome = if timed_out {
            // §9.3 — fallback to default action
            eprintln!(
                "\n[Timeout] No response within {}s — using default.",
                self.safety.config().decision_timeout_ms / 1000
            );
            resolve_default(&event, DecisionSource::Timeout)
        } else {
            let resp = match try_parse_json_response(&raw) {
                Ok(resp) => resp,
                Err(err) if self.debug_plain_text => {
                    let outcome = resolve_plain(&event, &raw, DecisionSource::User)?;
                    if let Some(logger) = &self.logger {
                        logger.append(&RunLogEntry::Decision {
                            request_id: event.request_id,
                            step_id: event.step_id.clone(),
                            outcome: outcome.clone(),
                            timestamp: timestamp_now(),
                        })?;
                    }
                    return Ok(outcome);
                }
                Err(err) => return Err(err),
            };
            validate_response_identity(&event, &resp)?;
            if let Some(logger) = &self.logger {
                if let Ok(resp_json) = serde_json::to_value(&resp) {
                    let _ = logger.append(&RunLogEntry::Response {
                        response: resp_json,
                        timestamp: timestamp_now(),
                    });
                }
            }
            validate_and_resolve(&event, resp)?
        };

        // ── 5. Log the canonical decision (§11) ───────────────────────────────
        if let Some(logger) = &self.logger {
            logger.append(&RunLogEntry::Decision {
                request_id: event.request_id,
                step_id: event.step_id.clone(),
                outcome: outcome.clone(),
                timestamp: timestamp_now(),
            })?;
        }

        Ok(outcome)
    }
}

impl StdioControlExecutor {
    fn abort_for_safety_limit(
        &self,
        event: &ControlEvent,
        limit: SafetyLimitTriggered,
    ) -> Result<ControlOutcome, ControlError> {
        let outcome = forced_abort();
        if let Some(logger) = &self.logger {
            logger.append(&RunLogEntry::SafetyLimitTriggered {
                safety: limit.clone(),
                timestamp: timestamp_now(),
            })?;
            logger.append(&RunLogEntry::RunFailed {
                event: "run_failed".to_string(),
                run_id: event.run_id.clone(),
                step_id: Some(event.step_id.clone()),
                reason: limit.abort_reason,
                timestamp: timestamp_now(),
            })?;
            logger.append(&RunLogEntry::Decision {
                request_id: event.request_id,
                step_id: event.step_id.clone(),
                outcome: outcome.clone(),
                timestamp: timestamp_now(),
            })?;
        }
        Ok(outcome)
    }
}

// ── ReplayControlExecutor ──────────────────────────────────────────────────────

/// Deterministic replay executor (§11, §14).
///
/// Reads pre-recorded [`RunLogEntry::Decision`] entries from a run log and
/// replays them in order, matching on `request_id`.  Guarantees that the same
/// response sequence produces the same execution path (§14 — determinism).
pub struct ReplayControlExecutor {
    entries: Vec<RunLogEntry>,
    pos: usize,
}

impl ReplayControlExecutor {
    /// Load from an existing run log file.
    pub fn from_log(path: &Path) -> Result<Self, ControlError> {
        let logger = RunLogger {
            path: path.to_path_buf(),
        };
        let entries = logger.read_all()?;
        Ok(Self { entries, pos: 0 })
    }

    /// Build directly from a vector of entries (useful in tests).
    pub fn from_entries(entries: Vec<RunLogEntry>) -> Self {
        Self { entries, pos: 0 }
    }
}

impl ControlExecutor for ReplayControlExecutor {
    fn emit(&mut self, event: ControlEvent) -> Result<ControlOutcome, ControlError> {
        let request_id = event.request_id;

        // Scan forward for the matching Decision entry
        while self.pos < self.entries.len() {
            let entry = &self.entries[self.pos];
            self.pos += 1;

            if let RunLogEntry::Decision {
                request_id: rid,
                step_id,
                outcome,
                ..
            } = entry
            {
                if *rid != request_id {
                    continue;
                }
                if step_id != &event.step_id {
                    return Err(ControlError::StepIdMismatch {
                        expected: event.step_id.clone(),
                        got: step_id.clone(),
                    });
                }
                validate_outcome_type(&event, outcome)?;
                return Ok(outcome.clone());
            }
        }

        // No matching entry — fall back to default (§9.3)
        Ok(resolve_default(&event, DecisionSource::Default))
    }
}

// ── NullControlExecutor ───────────────────────────────────────────────────────

/// No-op executor that immediately returns the `default` action.
///
/// Useful in non-interactive pipelines where control events should be
/// silently resolved by their defaults without blocking.
pub struct NullControlExecutor;

impl ControlExecutor for NullControlExecutor {
    fn emit(&mut self, event: ControlEvent) -> Result<ControlOutcome, ControlError> {
        Ok(resolve_default(&event, DecisionSource::Default))
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Print the CLI prompt to stderr (§12, UI-independent per §14).
fn print_control_event_prompt(event: &ControlEvent) {
    let stderr = std::io::stderr();
    let mut out = stderr.lock();
    let _ = writeln!(out);
    match &event.payload {
        ControlPayload::Decision {
            reason,
            options,
            default,
            ..
        } => {
            let _ = writeln!(out, "[Decision Required]");
            let _ = writeln!(out, "Reason: {reason}");
            let opts: Vec<&str> = options.iter().map(|o| o.as_str()).collect();
            let _ = writeln!(out, "Options: {}", opts.join(" / "));
            let _ = writeln!(out, "Default: {}", default.as_str());
        }
        ControlPayload::Input { prompt, .. } => {
            let _ = writeln!(out, "[Input Required]");
            let _ = writeln!(out, "{prompt}");
        }
        ControlPayload::Approval {
            action,
            risk,
            diff,
            files,
        } => {
            let _ = writeln!(out, "[Approval Required]");
            let _ = writeln!(out, "Action: {action}");
            let _ = writeln!(out, "Risk:   {}", risk.as_str());
            if !files.is_empty() {
                let _ = writeln!(out, "Files:  {}", files.join(", "));
            }
            if !diff.is_empty() {
                let _ = writeln!(out, "---");
                let _ = writeln!(out, "{diff}");
                let _ = writeln!(out, "---");
            }
            let _ = writeln!(out, "Options: modify / abort");
        }
    }
    let _ = write!(out, "> ");
    let _ = out.flush();
}

/// Attempt to parse the raw input as a JSON [`ControlResponse`].
fn try_parse_json_response(raw: &str) -> Result<ControlResponse, ControlError> {
    if !raw.trim_start().starts_with('{') {
        return Err(ControlError::ParseError(
            "control responses must be JSON; plain text is debug-only".to_string(),
        ));
    }
    serde_json::from_str(raw).map_err(|err| ControlError::ParseError(err.to_string()))
}

fn validate_response_identity(
    event: &ControlEvent,
    resp: &ControlResponse,
) -> Result<(), ControlError> {
    if resp.request_id != event.request_id {
        return Err(ControlError::RequestIdMismatch {
            expected: event.request_id,
            got: resp.request_id,
        });
    }
    if resp.step_id != event.step_id {
        return Err(ControlError::StepIdMismatch {
            expected: event.step_id.clone(),
            got: resp.step_id.clone(),
        });
    }
    if resp.response_to != event.event {
        return Err(ControlError::ResponseTypeMismatch {
            expected: event.event,
            got: resp.response_to,
        });
    }
    Ok(())
}

/// Validate a JSON response against the emitted event and produce an outcome.
///
/// Enforces:
/// - action is in the allowlist (§10)
/// - action is in the options list for `decision_required`
fn validate_and_resolve(
    event: &ControlEvent,
    resp: ControlResponse,
) -> Result<ControlOutcome, ControlError> {
    match &event.payload {
        ControlPayload::Decision { options, .. } => {
            let action = resp
                .action
                .ok_or_else(|| ControlError::ParseError("missing action".to_string()))?;
            // §10 — must also be in the per-event options
            if !options.contains(&action) {
                return Err(ControlError::UnknownAction(action.as_str().to_string()));
            }
            Ok(ControlOutcome::Decision {
                action,
                source: DecisionSource::User,
            })
        }
        ControlPayload::Input { .. } => {
            let data = resp
                .data
                .unwrap_or(serde_json::Value::String(String::new()));
            Ok(ControlOutcome::Input {
                data,
                source: DecisionSource::User,
            })
        }
        ControlPayload::Approval { .. } => {
            let action = resp
                .action
                .ok_or_else(|| ControlError::ParseError("missing action".to_string()))?;
            if !matches!(action, DecisionAction::Modify | DecisionAction::Abort) {
                return Err(ControlError::UnknownAction(action.as_str().to_string()));
            }
            Ok(ControlOutcome::Decision {
                action,
                source: DecisionSource::User,
            })
        }
    }
}

/// Resolve a plain-text (non-JSON) input string against the emitted event.
fn resolve_plain(
    event: &ControlEvent,
    raw: &str,
    source: DecisionSource,
) -> Result<ControlOutcome, ControlError> {
    match &event.payload {
        ControlPayload::Decision { options, .. } => {
            // §10 — must be in global allowlist AND options list
            let action = DecisionAction::parse(raw)
                .ok_or_else(|| ControlError::UnknownAction(raw.to_string()))?;
            if !options.contains(&action) {
                return Err(ControlError::UnknownAction(raw.to_string()));
            }
            Ok(ControlOutcome::Decision { action, source })
        }
        ControlPayload::Input { .. } => Ok(ControlOutcome::Input {
            data: serde_json::Value::String(raw.to_string()),
            source,
        }),
        ControlPayload::Approval { .. } => {
            let action = DecisionAction::parse(raw)
                .ok_or_else(|| ControlError::UnknownAction(raw.to_string()))?;
            if !matches!(action, DecisionAction::Modify | DecisionAction::Abort) {
                return Err(ControlError::UnknownAction(raw.to_string()));
            }
            Ok(ControlOutcome::Decision { action, source })
        }
    }
}

/// Apply the `default` action (§9.3).
fn resolve_default(event: &ControlEvent, source: DecisionSource) -> ControlOutcome {
    match &event.payload {
        ControlPayload::Decision { default, .. } => ControlOutcome::Decision {
            action: *default,
            source,
        },
        ControlPayload::Input { .. } => ControlOutcome::Input {
            data: serde_json::Value::Null,
            source,
        },
        // Safe default for approval: abort (never silently proceed).
        ControlPayload::Approval { .. } => ControlOutcome::Decision {
            action: DecisionAction::Abort,
            source,
        },
    }
}

fn forced_abort() -> ControlOutcome {
    ControlOutcome::Decision {
        action: DecisionAction::Abort,
        source: DecisionSource::Default,
    }
}

fn validate_outcome_type(
    event: &ControlEvent,
    outcome: &ControlOutcome,
) -> Result<(), ControlError> {
    match (&event.payload, outcome) {
        (ControlPayload::Input { .. }, ControlOutcome::Input { .. }) => Ok(()),
        (
            ControlPayload::Decision { .. } | ControlPayload::Approval { .. },
            ControlOutcome::Decision { .. },
        ) => Ok(()),
        _ => Err(ControlError::ParseError(
            "replay outcome type does not match control event type".to_string(),
        )),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_event::{ControlEventKind, DecisionReason, RiskLevel};
    use serde_json::json;
    use tempfile::tempdir;

    fn req(n: u128) -> RequestId {
        RequestId::from_u128(0x018f_6a2d_1b2c_7abc_8def_000000000000 + n)
    }

    fn decision_event() -> ControlEvent {
        ControlEvent::decision_required(
            "run-001",
            "step-2",
            req(1),
            DecisionReason::ValidationFailed.as_str(),
            json!({"message": "type mismatch"}),
            vec![
                DecisionAction::Retry,
                DecisionAction::Skip,
                DecisionAction::Abort,
            ],
            DecisionAction::Abort,
        )
    }

    fn approval_event() -> ControlEvent {
        ControlEvent::approval_required(
            "run-001",
            "step-3",
            req(2),
            "apply_patch",
            RiskLevel::Medium,
            "diff content",
            vec!["src/main.rs".to_string()],
        )
    }

    fn input_event() -> ControlEvent {
        ControlEvent::input_required(
            "run-001",
            "step-1",
            req(3),
            "Specify target file",
            json!({"type": "string"}),
            true,
        )
    }

    // ── NullControlExecutor ────────────────────────────────────────────────────

    #[test]
    fn null_executor_returns_default_for_decision() {
        let mut exec = NullControlExecutor;
        let outcome = exec.emit(decision_event()).unwrap();
        assert_eq!(outcome.action(), Some("abort")); // default
        assert!(!outcome.timed_out());
    }

    #[test]
    fn null_executor_returns_reject_for_approval() {
        let mut exec = NullControlExecutor;
        let outcome = exec.emit(approval_event()).unwrap();
        assert_eq!(outcome.action(), Some("abort")); // safe default
    }

    #[test]
    fn null_executor_returns_null_for_input() {
        let mut exec = NullControlExecutor;
        let outcome = exec.emit(input_event()).unwrap();
        if let ControlOutcome::Input { data, source } = outcome {
            assert_eq!(data, serde_json::Value::Null);
            assert_eq!(source, DecisionSource::Default);
        } else {
            panic!("expected Input outcome");
        }
    }

    // ── ReplayControlExecutor ─────────────────────────────────────────────────

    #[test]
    fn replay_executor_matches_by_request_id() {
        let entries = vec![
            RunLogEntry::Decision {
                request_id: req(1),
                step_id: "step-2".to_string(),
                outcome: ControlOutcome::Decision {
                    action: DecisionAction::Retry,
                    source: DecisionSource::User,
                },
                timestamp: "2026-04-29T00:00:00Z".to_string(),
            },
            RunLogEntry::Decision {
                request_id: req(2),
                step_id: "step-3".to_string(),
                outcome: ControlOutcome::Decision {
                    action: DecisionAction::Modify,
                    source: DecisionSource::User,
                },
                timestamp: "2026-04-29T00:00:01Z".to_string(),
            },
        ];
        let mut exec = ReplayControlExecutor::from_entries(entries);

        let o1 = exec.emit(decision_event()).unwrap();
        assert_eq!(o1.action(), Some("retry"));

        let o2 = exec.emit(approval_event()).unwrap();
        assert_eq!(o2.action(), Some("modify"));
    }

    #[test]
    fn replay_executor_skips_non_decision_entries() {
        let event_json = serde_json::to_value(decision_event()).unwrap();
        let entries = vec![
            RunLogEntry::Event {
                event: event_json,
                timestamp: "2026-04-29T00:00:00Z".to_string(),
            },
            RunLogEntry::Decision {
                request_id: req(1),
                step_id: "step-2".to_string(),
                outcome: ControlOutcome::Decision {
                    action: DecisionAction::Skip,
                    source: DecisionSource::User,
                },
                timestamp: "2026-04-29T00:00:01Z".to_string(),
            },
        ];
        let mut exec = ReplayControlExecutor::from_entries(entries);
        let o = exec.emit(decision_event()).unwrap();
        assert_eq!(o.action(), Some("skip"));
    }

    #[test]
    fn replay_executor_falls_back_to_default_when_no_entry() {
        let mut exec = ReplayControlExecutor::from_entries(vec![]);
        let outcome = exec.emit(decision_event()).unwrap();
        assert_eq!(outcome.action(), Some("abort")); // default
        assert_eq!(outcome.source(), DecisionSource::Default);
    }

    #[test]
    fn replay_executor_preserves_timeout_source() {
        let entries = vec![RunLogEntry::Decision {
            request_id: req(1),
            step_id: "step-2".to_string(),
            outcome: ControlOutcome::Decision {
                action: DecisionAction::Abort,
                source: DecisionSource::Timeout,
            },
            timestamp: "2026-04-29T00:00:00Z".to_string(),
        }];
        let mut exec = ReplayControlExecutor::from_entries(entries);
        let outcome = exec.emit(decision_event()).unwrap();
        assert!(outcome.timed_out());
        assert_eq!(outcome.source(), DecisionSource::Timeout);
    }

    #[test]
    fn replay_executor_replays_safety_abort_decision() {
        let entries = vec![
            RunLogEntry::SafetyLimitTriggered {
                safety: SafetyLimitTriggered {
                    event: "safety_limit_triggered".to_string(),
                    limit_type: SafetyLimitType::MaxAttempts,
                    run_id: "run-001".to_string(),
                    step_id: Some("step-2".to_string()),
                    attempts: Some(4),
                    loop_count: Some(4),
                    abort_reason: "step step-2 exceeded max_attempts 3".to_string(),
                },
                timestamp: "2026-04-29T00:00:00Z".to_string(),
            },
            RunLogEntry::RunFailed {
                event: "run_failed".to_string(),
                run_id: "run-001".to_string(),
                step_id: Some("step-2".to_string()),
                reason: "step step-2 exceeded max_attempts 3".to_string(),
                timestamp: "2026-04-29T00:00:01Z".to_string(),
            },
            RunLogEntry::Decision {
                request_id: req(1),
                step_id: "step-2".to_string(),
                outcome: forced_abort(),
                timestamp: "2026-04-29T00:00:02Z".to_string(),
            },
        ];
        let mut exec = ReplayControlExecutor::from_entries(entries);
        let outcome = exec.emit(decision_event()).unwrap();
        assert_eq!(outcome.action(), Some("abort"));
        assert_eq!(outcome.source(), DecisionSource::Default);
    }

    // ── RunLogger ──────────────────────────────────────────────────────────────

    #[test]
    fn run_logger_creates_and_appends() {
        let dir = tempdir().unwrap();
        let logger = RunLogger::new(dir.path(), "run-test").unwrap();
        logger
            .append(&RunLogEntry::Decision {
                request_id: req(1),
                step_id: "step-2".to_string(),
                outcome: ControlOutcome::Decision {
                    action: DecisionAction::Retry,
                    source: DecisionSource::User,
                },
                timestamp: "2026-04-29T00:00:00Z".to_string(),
            })
            .unwrap();
        let entries = logger.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn run_logger_roundtrips_all_entry_kinds() {
        let dir = tempdir().unwrap();
        let logger = RunLogger::new(dir.path(), "run-rt").unwrap();

        logger
            .append(&RunLogEntry::Event {
                event: json!({"event": "decision_required"}),
                timestamp: "2026-04-29T00:00:00Z".to_string(),
            })
            .unwrap();
        logger
            .append(&RunLogEntry::Response {
                response: json!({"action": "retry"}),
                timestamp: "2026-04-29T00:00:01Z".to_string(),
            })
            .unwrap();
        logger
            .append(&RunLogEntry::Decision {
                request_id: req(1),
                step_id: "step-2".to_string(),
                outcome: ControlOutcome::Decision {
                    action: DecisionAction::Retry,
                    source: DecisionSource::User,
                },
                timestamp: "2026-04-29T00:00:02Z".to_string(),
            })
            .unwrap();
        logger
            .append(&RunLogEntry::SafetyLimitTriggered {
                safety: SafetyLimitTriggered {
                    event: "safety_limit_triggered".to_string(),
                    limit_type: SafetyLimitType::MaxLoops,
                    run_id: "run-001".to_string(),
                    step_id: Some("step-2".to_string()),
                    attempts: Some(2),
                    loop_count: Some(6),
                    abort_reason: "run run-001 exceeded max_loops 5".to_string(),
                },
                timestamp: "2026-04-29T00:00:03Z".to_string(),
            })
            .unwrap();
        logger
            .append(&RunLogEntry::SafetySnapshot {
                safety: SafetySnapshot {
                    event: "safety_snapshot".to_string(),
                    run_id: "run-001".to_string(),
                    step_id: "step-2".to_string(),
                    attempts: 2,
                    max_attempts: 3,
                    loop_count: 2,
                    max_loops: 5,
                },
                timestamp: "2026-04-29T00:00:04Z".to_string(),
            })
            .unwrap();

        let entries = logger.read_all().unwrap();
        assert_eq!(entries.len(), 5);
        assert!(matches!(entries[0], RunLogEntry::Event { .. }));
        assert!(matches!(entries[1], RunLogEntry::Response { .. }));
        assert!(matches!(entries[2], RunLogEntry::Decision { .. }));
        assert!(matches!(
            entries[3],
            RunLogEntry::SafetyLimitTriggered { .. }
        ));
        assert!(matches!(entries[4], RunLogEntry::SafetySnapshot { .. }));
    }

    // ── Safety Layer ──────────────────────────────────────────────────────────

    #[test]
    fn safety_config_enforces_bounds() {
        assert!(SafetyConfig::new(0, DEFAULT_MAX_LOOPS, DEFAULT_TIMEOUT_MS).is_err());
        assert!(SafetyConfig::new(11, DEFAULT_MAX_LOOPS, DEFAULT_TIMEOUT_MS).is_err());
        assert!(SafetyConfig::new(DEFAULT_MAX_ATTEMPTS, 0, DEFAULT_TIMEOUT_MS).is_err());
        assert!(SafetyConfig::new(DEFAULT_MAX_ATTEMPTS, 21, DEFAULT_TIMEOUT_MS).is_err());
        assert!(SafetyConfig::new(DEFAULT_MAX_ATTEMPTS, DEFAULT_MAX_LOOPS, 999).is_err());
        assert_eq!(
            SafetyConfig::new(3, 5, 30_000).unwrap(),
            SafetyConfig::default()
        );
    }

    #[test]
    fn safety_layer_aborts_after_max_attempts() {
        let config = SafetyConfig::new(3, 20, 30_000).unwrap();
        let mut safety = ControlSafetyLayer::new(config);
        let event = decision_event();
        let snapshot = safety.record_event(&event).unwrap();
        assert_eq!(snapshot.attempts, 1);
        assert_eq!(snapshot.loop_count, 1);
        assert!(safety.record_event(&event).is_ok());
        assert!(safety.record_event(&event).is_ok());
        let limit = safety.record_event(&event).unwrap_err();
        assert_eq!(limit.limit_type, SafetyLimitType::MaxAttempts);
        assert_eq!(limit.attempts, Some(4));
        assert_eq!(limit.loop_count, Some(4));
    }

    #[test]
    fn safety_layer_aborts_after_max_loops() {
        let config = SafetyConfig::new(10, 2, 30_000).unwrap();
        let mut safety = ControlSafetyLayer::new(config);
        assert!(safety.record_event(&decision_event()).is_ok());
        assert!(safety.record_event(&approval_event()).is_ok());
        let limit = safety.record_event(&input_event()).unwrap_err();
        assert_eq!(limit.limit_type, SafetyLimitType::MaxLoops);
        assert_eq!(limit.loop_count, Some(3));
    }

    #[test]
    fn safety_layer_rejects_run_id_mismatch() {
        let mut safety = ControlSafetyLayer::default();
        assert!(safety.record_event(&decision_event()).is_ok());
        let other = ControlEvent::decision_required(
            "run-002",
            "step-2",
            req(4),
            DecisionReason::Conflict.as_str(),
            json!({}),
            vec![DecisionAction::Abort],
            DecisionAction::Abort,
        );
        let limit = safety.record_event(&other).unwrap_err();
        assert_eq!(limit.limit_type, SafetyLimitType::InvalidState);
    }

    // ── validate_and_resolve ───────────────────────────────────────────────────

    #[test]
    fn validate_rejects_unknown_action() {
        let resp = ControlResponse {
            response_to: ControlEventKind::DecisionRequired,
            request_id: req(1),
            run_id: "run-001".to_string(),
            step_id: "step-2".to_string(),
            action: None,
            data: None,
        };
        let err = validate_and_resolve(&decision_event(), resp).unwrap_err();
        assert!(matches!(err, ControlError::ParseError(_)));
    }

    #[test]
    fn validate_rejects_action_not_in_options() {
        // "modify" is in the global allowlist but NOT in the options of decision_event()
        let resp = ControlResponse {
            response_to: ControlEventKind::DecisionRequired,
            request_id: req(1),
            run_id: "run-001".to_string(),
            step_id: "step-2".to_string(),
            action: Some(DecisionAction::Modify),
            data: None,
        };
        let err = validate_and_resolve(&decision_event(), resp).unwrap_err();
        assert!(matches!(err, ControlError::UnknownAction(_)));
    }

    #[test]
    fn validate_accepts_valid_decision() {
        let resp = ControlResponse {
            response_to: ControlEventKind::DecisionRequired,
            request_id: req(1),
            run_id: "run-001".to_string(),
            step_id: "step-2".to_string(),
            action: Some(DecisionAction::Retry),
            data: None,
        };
        let outcome = validate_and_resolve(&decision_event(), resp).unwrap();
        assert_eq!(outcome.action(), Some("retry"));
    }

    #[test]
    fn validate_rejects_response_identity_mismatch() {
        let resp = ControlResponse {
            response_to: ControlEventKind::InputRequired,
            request_id: req(2),
            run_id: "run-001".to_string(),
            step_id: "wrong-step".to_string(),
            action: Some(DecisionAction::Retry),
            data: None,
        };
        let event = decision_event();
        let err = validate_response_identity(&event, &resp).unwrap_err();
        assert!(matches!(err, ControlError::RequestIdMismatch { .. }));

        let resp = ControlResponse {
            request_id: event.request_id,
            ..resp
        };
        let err = validate_response_identity(&event, &resp).unwrap_err();
        assert!(matches!(err, ControlError::StepIdMismatch { .. }));

        let resp = ControlResponse {
            response_to: ControlEventKind::InputRequired,
            step_id: event.step_id.clone(),
            ..resp
        };
        let err = validate_response_identity(&event, &resp).unwrap_err();
        assert!(matches!(err, ControlError::ResponseTypeMismatch { .. }));
    }

    #[test]
    fn plain_text_is_rejected_outside_debug_mode() {
        let err = try_parse_json_response("retry").unwrap_err();
        assert!(matches!(err, ControlError::ParseError(_)));
    }

    #[test]
    fn validate_accepts_modify_for_approval() {
        let resp = ControlResponse {
            response_to: ControlEventKind::ApprovalRequired,
            request_id: req(2),
            run_id: "run-001".to_string(),
            step_id: "step-3".to_string(),
            action: Some(DecisionAction::Modify),
            data: None,
        };
        let outcome = validate_and_resolve(&approval_event(), resp).unwrap();
        assert!(outcome.is_approved());
    }

    #[test]
    fn validate_rejects_bad_approval_action() {
        let resp = ControlResponse {
            response_to: ControlEventKind::ApprovalRequired,
            request_id: req(2),
            run_id: "run-001".to_string(),
            step_id: "step-3".to_string(),
            action: Some(DecisionAction::Retry),
            data: None,
        };
        let err = validate_and_resolve(&approval_event(), resp).unwrap_err();
        assert!(matches!(err, ControlError::UnknownAction(_)));
    }

    // ── resolve_plain ──────────────────────────────────────────────────────────

    #[test]
    fn plain_input_accepted_for_input_event() {
        let outcome = resolve_plain(&input_event(), "src/main.rs", DecisionSource::User).unwrap();
        if let ControlOutcome::Input { data, .. } = outcome {
            assert_eq!(data, serde_json::Value::String("src/main.rs".to_string()));
        } else {
            panic!("expected Input outcome");
        }
    }

    #[test]
    fn plain_unknown_action_rejected_for_decision() {
        let err = resolve_plain(&decision_event(), "nope", DecisionSource::User).unwrap_err();
        assert!(matches!(err, ControlError::UnknownAction(_)));
    }
}
