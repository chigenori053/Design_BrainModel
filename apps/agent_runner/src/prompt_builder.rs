use design_cli::control_event::{ControlEvent, ControlPayload, DecisionAction};
use serde::{Deserialize, Serialize};

use crate::session::Session;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConstraints {
    pub allowed_actions: Vec<DecisionAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInput {
    pub task: String,
    pub event: ControlEvent,
    pub history: Vec<serde_json::Value>,
    pub constraints: AgentConstraints,
}

#[derive(Debug, Clone, Default)]
pub struct PromptBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPromptContext {
    pub attempt: u8,
    pub max_attempts: u8,
    pub reason: String,
}

impl PromptBuilder {
    pub fn build_input(&self, event: &ControlEvent, session: &Session) -> AgentInput {
        AgentInput {
            task: session.task.clone(),
            event: event.clone(),
            history: session.compressed_history(),
            constraints: AgentConstraints {
                allowed_actions: allowed_actions(event),
            },
        }
    }

    pub fn build(&self, event: &ControlEvent, session: &Session) -> Result<String, String> {
        self.build_with_retry(event, session, None)
    }

    pub fn build_retry(
        &self,
        event: &ControlEvent,
        session: &Session,
        retry: &RetryPromptContext,
    ) -> Result<String, String> {
        self.build_with_retry(event, session, Some(retry))
    }

    fn build_with_retry(
        &self,
        event: &ControlEvent,
        session: &Session,
        retry: Option<&RetryPromptContext>,
    ) -> Result<String, String> {
        let input = self.build_input(event, session);
        let event_json =
            serde_json::to_string_pretty(&input.event).map_err(|err| err.to_string())?;
        let history_json =
            serde_json::to_string_pretty(&input.history).map_err(|err| err.to_string())?;
        let event_details_json =
            serde_json::to_string_pretty(&event_details(event)).map_err(|err| err.to_string())?;
        let schema = serde_json::json!({
            "response_to": event.event.as_str(),
            "request_id": event.request_id,
            "action": input.constraints.allowed_actions,
            "data": {}
        });
        let schema_json = serde_json::to_string_pretty(&schema).map_err(|err| err.to_string())?;
        let retry_instruction = retry
            .map(|retry| {
                format!(
                    "\nPrevious response failure:\n{}\nFix the format and semantics strictly.\nRetry attempt: {}/{}\n",
                    retry.reason, retry.attempt, retry.max_attempts
                )
            })
            .unwrap_or_default();
        Ok(format!(
            "System:\nYou are a decision engine. Decide only the response for the current DBM Control Event.\n\nContext:\nTask:\n{}\n\nCurrent Event:\n{}\n\nHistory (compressed):\n{}\n\nEvent Details:\n{}\n\nInstruction:\n- Choose only from allowed actions.\n- Return JSON only.\n- Do not include explanations.\n- Preserve request_id exactly.\n{}\nOutput:\n{}",
            input.task,
            event_json,
            history_json,
            event_details_json,
            retry_instruction,
            schema_json
        ))
    }
}

pub fn allowed_actions(event: &ControlEvent) -> Vec<DecisionAction> {
    match &event.payload {
        ControlPayload::Decision { options, .. } => options.clone(),
        ControlPayload::Input { .. } => Vec::new(),
        ControlPayload::Approval { .. } => vec![DecisionAction::Modify, DecisionAction::Abort],
    }
}

fn event_details(event: &ControlEvent) -> serde_json::Value {
    match &event.payload {
        ControlPayload::Decision {
            reason,
            context,
            options,
            default,
        } => serde_json::json!({
            "event_type": "decision_required",
            "reason": reason,
            "context": context,
            "options": options,
            "default": default,
            "risk": context.get("risk").cloned().unwrap_or(serde_json::Value::Null)
        }),
        ControlPayload::Input {
            prompt,
            schema,
            required,
        } => serde_json::json!({
            "event_type": "input_required",
            "prompt": prompt,
            "schema": schema,
            "required": required,
            "example": example_for_schema(schema)
        }),
        ControlPayload::Approval {
            action,
            risk,
            diff,
            files,
        } => serde_json::json!({
            "event_type": "approval_required",
            "action": action,
            "risk": risk,
            "files": files,
            "diff_summary": summarize_diff(diff)
        }),
    }
}

fn example_for_schema(schema: &serde_json::Value) -> serde_json::Value {
    match schema.get("type").and_then(|value| value.as_str()) {
        Some("object") => serde_json::json!({}),
        Some("array") => serde_json::json!([]),
        Some("string") => serde_json::json!(""),
        Some("integer" | "number") => serde_json::json!(0),
        Some("boolean") => serde_json::json!(false),
        _ => serde_json::Value::Null,
    }
}

fn summarize_diff(diff: &str) -> String {
    const MAX_LINES: usize = 12;
    const MAX_CHARS: usize = 1200;

    let mut summary = diff.lines().take(MAX_LINES).collect::<Vec<_>>().join("\n");
    if summary.len() > MAX_CHARS {
        summary.truncate(MAX_CHARS);
    }
    if diff.lines().count() > MAX_LINES || diff.len() > summary.len() {
        summary.push_str("\n...");
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_cli::control_event::{DecisionReason, RequestId};

    #[test]
    fn retry_prompt_includes_failure_reason_and_attempt() {
        let event = ControlEvent::decision_required(
            "run",
            "step",
            RequestId::from_u128(1),
            DecisionReason::Ambiguity.as_str(),
            serde_json::json!({ "risk": "medium" }),
            vec![DecisionAction::Retry, DecisionAction::Abort],
            DecisionAction::Abort,
        );
        let session = Session::new("run", "fix it");
        let retry = RetryPromptContext {
            attempt: 2,
            max_attempts: 3,
            reason: "Previous response was invalid JSON.".to_string(),
        };

        let prompt = PromptBuilder.build_retry(&event, &session, &retry).unwrap();

        assert!(prompt.contains("Previous response was invalid JSON."));
        assert!(prompt.contains("Retry attempt: 2/3"));
        assert!(prompt.contains("\"options\""));
        assert!(prompt.contains("\"default\""));
        assert!(prompt.contains("\"risk\""));
    }
}
