//! DBM-CLI-CONTROL-EVENT-SPEC v1.1 — Type System
//!
//! Control Event Protocol: externalises decision-making to an Agent
//! during autonomous execution without blocking the DBM execution model.
//!
//! # Event kinds
//! - `decision_required` — branch decision needed (fail / ambiguity / conflict)
//! - `input_required`    — additional input is missing from the agent
//! - `approval_required` — risky side-effect needs explicit approval
//!
//! The Executor blocks on every Control Event until a [`ControlResponse`] is
//! received, or the timeout expires and the `default` action is applied (§9).
//!
//! All events and responses are logged to `.dbm/runs/<run_id>.jsonl` (§11).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type RequestId = Uuid;

pub fn new_request_id() -> RequestId {
    Uuid::now_v7()
}

#[derive(Debug, Default)]
pub struct RequestIdRegistry {
    seen: BTreeSet<RequestId>,
}

impl RequestIdRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reserve(&mut self, request_id: RequestId) -> bool {
        self.seen.insert(request_id)
    }

    pub fn next_request_id(&mut self) -> RequestId {
        loop {
            let request_id = new_request_id();
            if self.reserve(request_id) {
                return request_id;
            }
        }
    }
}

// ── DecisionReason ─────────────────────────────────────────────────────────────

/// Why a `decision_required` event was emitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    ValidationFailed,
    Ambiguity,
    Conflict,
}

impl DecisionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ValidationFailed => "validation_failed",
            Self::Ambiguity => "ambiguity",
            Self::Conflict => "conflict",
        }
    }
}

// ── DecisionAction ─────────────────────────────────────────────────────────────

/// Allowlisted actions for a `decision_required` response (§10).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Retry,
    Skip,
    Abort,
    Modify,
}

impl DecisionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Skip => "skip",
            Self::Abort => "abort",
            Self::Modify => "modify",
        }
    }

    /// Parse from a plain string. Returns `None` if the value is not in the
    /// allowlist — callers should reject unknown actions (§10).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "retry" => Some(Self::Retry),
            "skip" => Some(Self::Skip),
            "abort" => Some(Self::Abort),
            "modify" => Some(Self::Modify),
            _ => None,
        }
    }
}

// ── RiskLevel ──────────────────────────────────────────────────────────────────

/// Risk classification for an `approval_required` event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

// ── ControlEventKind ───────────────────────────────────────────────────────────

/// Discriminant for the three Control Event types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlEventKind {
    DecisionRequired,
    InputRequired,
    ApprovalRequired,
}

impl ControlEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DecisionRequired => "decision_required",
            Self::InputRequired => "input_required",
            Self::ApprovalRequired => "approval_required",
        }
    }
}

// ── ControlPayload ─────────────────────────────────────────────────────────────

/// Tagged union of the three event payloads. The `kind` field is required;
/// unknown kinds fail deserialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlPayload {
    Decision {
        reason: String,
        context: serde_json::Value,
        options: Vec<DecisionAction>,
        default: DecisionAction,
    },
    Input {
        prompt: String,
        schema: serde_json::Value,
        required: bool,
    },
    Approval {
        action: String,
        risk: RiskLevel,
        diff: String,
        files: Vec<String>,
    },
}

// ── ControlEvent ───────────────────────────────────────────────────────────────

/// A control event emitted by the Executor to the Agent (§4).
///
/// ```json
/// {
///   "event": "decision_required",
///   "phase": "control",
///   "run_id": "run-001",
///   "step_id": "step-2",
///   "timestamp": "2026-04-29T10:00:00Z",
///   "request_id": "req-001",
///   "payload": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlEvent {
    pub event: ControlEventKind,
    /// Always `"control"`.
    pub phase: String,
    pub run_id: String,
    pub step_id: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
    /// Unique per-request identifier used for response matching (§10).
    pub request_id: RequestId,
    pub payload: ControlPayload,
}

impl ControlEvent {
    /// Build a `decision_required` event.
    pub fn decision_required(
        run_id: impl Into<String>,
        step_id: impl Into<String>,
        request_id: RequestId,
        reason: impl Into<String>,
        context: serde_json::Value,
        options: Vec<DecisionAction>,
        default: DecisionAction,
    ) -> Self {
        Self {
            event: ControlEventKind::DecisionRequired,
            phase: "control".to_string(),
            run_id: run_id.into(),
            step_id: step_id.into(),
            timestamp: timestamp_now(),
            request_id,
            payload: ControlPayload::Decision {
                reason: reason.into(),
                context,
                options,
                default,
            },
        }
    }

    /// Build an `input_required` event.
    pub fn input_required(
        run_id: impl Into<String>,
        step_id: impl Into<String>,
        request_id: RequestId,
        prompt: impl Into<String>,
        schema: serde_json::Value,
        required: bool,
    ) -> Self {
        Self {
            event: ControlEventKind::InputRequired,
            phase: "control".to_string(),
            run_id: run_id.into(),
            step_id: step_id.into(),
            timestamp: timestamp_now(),
            request_id,
            payload: ControlPayload::Input {
                prompt: prompt.into(),
                schema,
                required,
            },
        }
    }

    /// Build an `approval_required` event.
    pub fn approval_required(
        run_id: impl Into<String>,
        step_id: impl Into<String>,
        request_id: RequestId,
        action: impl Into<String>,
        risk: RiskLevel,
        diff: impl Into<String>,
        files: Vec<String>,
    ) -> Self {
        Self {
            event: ControlEventKind::ApprovalRequired,
            phase: "control".to_string(),
            run_id: run_id.into(),
            step_id: step_id.into(),
            timestamp: timestamp_now(),
            request_id,
            payload: ControlPayload::Approval {
                action: action.into(),
                risk,
                diff: diff.into(),
                files,
            },
        }
    }
}

// ── ControlResponse ────────────────────────────────────────────────────────────

/// Agent response to a Control Event (§6).
///
/// ```json
/// {
///   "response_to": "decision_required",
///   "request_id": "req-001",
///   "run_id": "run-001",
///   "step_id": "step-2",
///   "action": "retry"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlResponse {
    pub response_to: ControlEventKind,
    pub request_id: RequestId,
    pub run_id: String,
    pub step_id: String,
    /// For `decision_required` and `approval_required` responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<DecisionAction>,
    /// For `input_required` responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ── ControlOutcome ─────────────────────────────────────────────────────────────

/// The resolved outcome after an agent response or timeout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    User,
    Default,
    Timeout,
}

/// The resolved outcome after an agent response or timeout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlOutcome {
    /// A named action was selected (for `decision_required` / `approval_required`).
    Decision {
        action: DecisionAction,
        source: DecisionSource,
    },
    /// External data was provided (for `input_required`).
    Input {
        data: serde_json::Value,
        source: DecisionSource,
    },
}

impl ControlOutcome {
    pub fn timed_out(&self) -> bool {
        self.source() == DecisionSource::Timeout
    }

    pub fn source(&self) -> DecisionSource {
        match self {
            Self::Decision { source, .. } | Self::Input { source, .. } => *source,
        }
    }

    /// The resolved action string, if any.
    pub fn action(&self) -> Option<&str> {
        match self {
            Self::Decision { action, .. } => Some(action.as_str()),
            Self::Input { .. } => None,
        }
    }

    /// Whether the resolved action equals `"abort"`.
    pub fn is_abort(&self) -> bool {
        self.action() == Some("abort")
    }

    /// Whether the resolved action equals `"retry"`.
    pub fn is_retry(&self) -> bool {
        self.action() == Some("retry")
    }

    /// Whether the resolved action proceeds with an approval-style event.
    pub fn is_approved(&self) -> bool {
        self.action() == Some("modify")
    }
}

// ── ControlError ──────────────────────────────────────────────────────────────

/// Errors that can occur while processing a Control Event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// Response `request_id` did not match the emitted event (§10).
    RequestIdMismatch {
        expected: RequestId,
        got: RequestId,
    },
    StepIdMismatch {
        expected: String,
        got: String,
    },
    ResponseTypeMismatch {
        expected: ControlEventKind,
        got: ControlEventKind,
    },
    /// The action value was not in the allowlist (§10).
    UnknownAction(String),
    /// The response JSON could not be parsed.
    ParseError(String),
    /// Safety or replay state is inconsistent and execution must abort.
    InvalidState(String),
    /// An I/O error occurred during emit or receive.
    IoError(String),
    /// The run log could not be written.
    LogError(String),
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequestIdMismatch { expected, got } => {
                write!(f, "request_id mismatch: expected {expected}, got {got}")
            }
            Self::StepIdMismatch { expected, got } => {
                write!(f, "step_id mismatch: expected {expected}, got {got}")
            }
            Self::ResponseTypeMismatch { expected, got } => {
                write!(
                    f,
                    "response type mismatch: expected {}, got {}",
                    expected.as_str(),
                    got.as_str()
                )
            }
            Self::UnknownAction(a) => write!(f, "unknown action: {a}"),
            Self::ParseError(e) => write!(f, "parse error: {e}"),
            Self::InvalidState(e) => write!(f, "invalid state: {e}"),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::LogError(e) => write!(f, "log error: {e}"),
        }
    }
}

impl std::error::Error for ControlError {}

// ── Timestamp ─────────────────────────────────────────────────────────────────

/// Returns the current time as an ISO 8601 UTC string (e.g. `2026-04-29T10:00:00Z`).
pub fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    unix_secs_to_iso8601(secs)
}

/// Convert UNIX epoch seconds to an ISO 8601 UTC string without external deps.
fn unix_secs_to_iso8601(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = (secs / 86400) as i64;
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Hinnant's algorithm — days-since-epoch to (year, month, day) in the
/// proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as i64;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as i64;
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request_id() -> RequestId {
        Uuid::parse_str("018f6a2d-1b2c-7abc-8def-111111111111").unwrap()
    }

    #[test]
    fn test_decision_required_roundtrip() {
        let event = ControlEvent::decision_required(
            "run-001",
            "step-2",
            request_id(),
            DecisionReason::ValidationFailed.as_str(),
            json!({"message": "type mismatch"}),
            vec![
                DecisionAction::Retry,
                DecisionAction::Skip,
                DecisionAction::Abort,
            ],
            DecisionAction::Abort,
        );
        let json = serde_json::to_string(&event).unwrap();
        let de: ControlEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.event, de.event);
        assert_eq!(event.request_id, de.request_id);
        assert_eq!(event.payload, de.payload);
    }

    #[test]
    fn test_approval_required_roundtrip() {
        let event = ControlEvent::approval_required(
            "run-001",
            "step-3",
            request_id(),
            "apply_patch",
            RiskLevel::Medium,
            "--- a/src/main.rs\n+++ b/src/main.rs",
            vec!["src/main.rs".to_string()],
        );
        let json = serde_json::to_string(&event).unwrap();
        let de: ControlEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(de.event, ControlEventKind::ApprovalRequired);
        if let ControlPayload::Approval { risk, files, .. } = de.payload {
            assert_eq!(risk, RiskLevel::Medium);
            assert_eq!(files, vec!["src/main.rs"]);
        } else {
            panic!("wrong payload variant");
        }
    }

    #[test]
    fn test_input_required_roundtrip() {
        let event = ControlEvent::input_required(
            "run-001",
            "step-1",
            request_id(),
            "Specify target file",
            json!({"type": "string"}),
            true,
        );
        let json = serde_json::to_string(&event).unwrap();
        let de: ControlEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(de.event, ControlEventKind::InputRequired);
    }

    #[test]
    fn test_payload_requires_known_kind() {
        let missing_kind = json!({
            "reason": "ambiguity",
            "context": {},
            "options": ["retry"],
            "default": "retry"
        });
        assert!(serde_json::from_value::<ControlPayload>(missing_kind).is_err());

        let unknown_kind = json!({
            "kind": "future_payload",
            "reason": "ambiguity"
        });
        assert!(serde_json::from_value::<ControlPayload>(unknown_kind).is_err());
    }

    #[test]
    fn test_new_request_id_is_uuid_v7() {
        let id = new_request_id();
        assert_eq!(id.get_version_num(), 7);
    }

    #[test]
    fn test_request_id_registry_rejects_run_collision() {
        let mut registry = RequestIdRegistry::new();
        let id = request_id();
        assert!(registry.reserve(id));
        assert!(!registry.reserve(id));
        assert_eq!(registry.next_request_id().get_version_num(), 7);
    }

    #[test]
    fn test_decision_action_allowlist() {
        assert_eq!(DecisionAction::parse("retry"), Some(DecisionAction::Retry));
        assert_eq!(DecisionAction::parse("skip"), Some(DecisionAction::Skip));
        assert_eq!(DecisionAction::parse("abort"), Some(DecisionAction::Abort));
        assert_eq!(
            DecisionAction::parse("modify"),
            Some(DecisionAction::Modify)
        );
        assert_eq!(DecisionAction::parse("unknown"), None);
        assert_eq!(DecisionAction::parse("approve"), None);
    }

    #[test]
    fn test_control_response_decision() {
        let resp = ControlResponse {
            response_to: ControlEventKind::DecisionRequired,
            request_id: request_id(),
            run_id: "run-001".to_string(),
            step_id: "step-2".to_string(),
            action: Some(DecisionAction::Retry),
            data: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let de: ControlResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(de.action, Some(DecisionAction::Retry));
        assert_eq!(de.data, None);
    }

    #[test]
    fn test_control_response_input() {
        let resp = ControlResponse {
            response_to: ControlEventKind::InputRequired,
            request_id: request_id(),
            run_id: "run-001".to_string(),
            step_id: "step-1".to_string(),
            action: None,
            data: Some(json!("src/main.rs")),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let de: ControlResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(de.data, Some(json!("src/main.rs")));
        assert_eq!(de.action, None);
    }

    #[test]
    fn test_control_outcome_helpers() {
        let outcome = ControlOutcome::Decision {
            action: DecisionAction::Abort,
            source: DecisionSource::User,
        };
        assert!(outcome.is_abort());
        assert!(!outcome.is_retry());
        assert!(!outcome.timed_out());

        let timeout_outcome = ControlOutcome::Decision {
            action: DecisionAction::Abort,
            source: DecisionSource::Timeout,
        };
        assert!(timeout_outcome.timed_out());
    }

    #[test]
    fn test_timestamp_format() {
        // epoch 0 → 1970-01-01T00:00:00Z
        assert_eq!(unix_secs_to_iso8601(0), "1970-01-01T00:00:00Z");
        // known date: 2024-01-01T00:00:00Z → 1704067200
        assert_eq!(unix_secs_to_iso8601(1_704_067_200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn test_event_phase_is_always_control() {
        let e = ControlEvent::decision_required(
            "r",
            "s",
            request_id(),
            "validation_failed",
            json!({}),
            vec![DecisionAction::Abort],
            DecisionAction::Abort,
        );
        assert_eq!(e.phase, "control");
    }
}
