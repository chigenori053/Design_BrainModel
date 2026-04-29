use design_cli::control_event::{
    ControlEvent, ControlEventKind, ControlPayload, ControlResponse, DecisionAction,
};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseMapperConfig {
    pub max_retries: u8,
}

impl Default for ResponseMapperConfig {
    fn default() -> Self {
        Self { max_retries: 2 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResponseMapper {
    config: ResponseMapperConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryErrorKind {
    Parse,
    Validation,
    Semantic,
    Agent,
}

impl RetryErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse_error",
            Self::Validation => "validation_error",
            Self::Semantic => "semantic_error",
            Self::Agent => "agent_error",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Parse => "Parse error",
            Self::Validation => "Validation failed",
            Self::Semantic => "Semantic validation failed",
            Self::Agent => "Agent call failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryError {
    pub kind: RetryErrorKind,
    pub message: String,
}

impl RetryError {
    pub fn new(kind: RetryErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for RetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for RetryError {}

impl ResponseMapper {
    pub fn new(config: ResponseMapperConfig) -> Self {
        Self { config }
    }

    pub fn max_retries(&self) -> u8 {
        self.config.max_retries
    }

    pub fn parse(&self, raw: &str, event: &ControlEvent) -> Result<ControlResponse, RetryError> {
        if !raw.trim_start().starts_with('{') {
            return Err(RetryError::new(
                RetryErrorKind::Parse,
                "agent output must be a JSON object",
            ));
        }
        let output: AgentOutput = serde_json::from_str(raw).map_err(|err| {
            RetryError::new(
                RetryErrorKind::Parse,
                format!("invalid agent response JSON: {err}"),
            )
        })?;
        if output.response_to != event.event {
            return Err(RetryError::new(
                RetryErrorKind::Validation,
                format!(
                    "response_to mismatch: expected {}, got {}",
                    event.event.as_str(),
                    output.response_to.as_str()
                ),
            ));
        }
        if output.request_id != event.request_id {
            return Err(RetryError::new(
                RetryErrorKind::Validation,
                format!(
                    "request_id mismatch: expected {}, got {}",
                    event.request_id, output.request_id
                ),
            ));
        }

        match &event.payload {
            ControlPayload::Decision { options, .. } => {
                let action = output.action.ok_or_else(|| {
                    RetryError::new(
                        RetryErrorKind::Validation,
                        "decision response requires action",
                    )
                })?;
                if !options.contains(&action) {
                    return Err(RetryError::new(
                        RetryErrorKind::Semantic,
                        format!("action {} is not allowed", action.as_str()),
                    ));
                }
                Ok(response(event, Some(action), None))
            }
            ControlPayload::Input { .. } => Ok(response(event, None, output.data)),
            ControlPayload::Approval { .. } => {
                let action = output.action.ok_or_else(|| {
                    RetryError::new(
                        RetryErrorKind::Validation,
                        "approval response requires action",
                    )
                })?;
                if !matches!(action, DecisionAction::Modify | DecisionAction::Abort) {
                    return Err(RetryError::new(
                        RetryErrorKind::Semantic,
                        format!("approval action {} is not allowed", action.as_str()),
                    ));
                }
                Ok(response(event, Some(action), None))
            }
        }
    }

    pub fn default_response(&self, event: &ControlEvent) -> ControlResponse {
        match &event.payload {
            ControlPayload::Decision { default, .. } => response(event, Some(*default), None),
            ControlPayload::Input { .. } => response(event, None, Some(serde_json::Value::Null)),
            ControlPayload::Approval { .. } => response(event, Some(DecisionAction::Abort), None),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentOutput {
    response_to: ControlEventKind,
    request_id: design_cli::control_event::RequestId,
    #[serde(default)]
    action: Option<DecisionAction>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

fn response(
    event: &ControlEvent,
    action: Option<DecisionAction>,
    data: Option<serde_json::Value>,
) -> ControlResponse {
    ControlResponse {
        response_to: event.event,
        request_id: event.request_id,
        run_id: event.run_id.clone(),
        step_id: event.step_id.clone(),
        action,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_cli::control_event::DecisionReason;

    fn req() -> design_cli::control_event::RequestId {
        design_cli::control_event::RequestId::from_u128(0x018f_6a2d_1b2c_7abc_8def_222222222222)
    }

    fn decision_event() -> ControlEvent {
        ControlEvent::decision_required(
            "run-1",
            "step-1",
            req(),
            DecisionReason::Conflict.as_str(),
            serde_json::json!({}),
            vec![DecisionAction::Retry, DecisionAction::Abort],
            DecisionAction::Abort,
        )
    }

    #[test]
    fn maps_valid_decision_response() {
        let event = decision_event();
        let raw = serde_json::json!({
            "response_to": "decision_required",
            "request_id": event.request_id,
            "action": "retry",
            "data": {}
        })
        .to_string();
        let response = ResponseMapper::default().parse(&raw, &event).unwrap();
        assert_eq!(response.action, Some(DecisionAction::Retry));
        assert_eq!(response.step_id, "step-1");
    }

    #[test]
    fn rejects_non_json_output() {
        let err = ResponseMapper::default()
            .parse("retry", &decision_event())
            .unwrap_err();
        assert_eq!(err.kind, RetryErrorKind::Parse);
        assert!(err.message.contains("JSON"));
    }

    #[test]
    fn rejects_request_id_mismatch() {
        let event = decision_event();
        let raw = serde_json::json!({
            "response_to": "decision_required",
            "request_id": design_cli::control_event::RequestId::from_u128(7),
            "action": "retry"
        })
        .to_string();
        let err = ResponseMapper::default().parse(&raw, &event).unwrap_err();
        assert_eq!(err.kind, RetryErrorKind::Validation);
        assert!(err.message.contains("request_id mismatch"));
    }

    #[test]
    fn classifies_disallowed_action_as_semantic_error() {
        let event = decision_event();
        let raw = serde_json::json!({
            "response_to": "decision_required",
            "request_id": event.request_id,
            "action": "skip"
        })
        .to_string();
        let err = ResponseMapper::default().parse(&raw, &event).unwrap_err();
        assert_eq!(err.kind, RetryErrorKind::Semantic);
    }
}
