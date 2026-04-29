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
    pub retry_count: u8,
    pub max_retries: u8,
    pub last_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptContext {
    pub task: String,
    pub event_summary: String,
    pub context: String,
    pub schema: String,
    pub retry_count: u8,
    pub max_retries: u8,
    pub last_error: Option<String>,
    pub allowed_actions: Vec<DecisionAction>,
    pub default_action: DecisionAction,
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
        let history_json =
            serde_json::to_string_pretty(&input.history).map_err(|err| err.to_string())?;
        let schema_json = response_schema_json(event, &input.constraints.allowed_actions)?;
        let prompt_context = PromptContext {
            task: input.task,
            event_summary: event_summary(event),
            context: format!(
                "Control event:\n{}\n\nHistory (compressed):\n{}",
                serde_json::to_string_pretty(&input.event).map_err(|err| err.to_string())?,
                history_json
            ),
            schema: schema_json,
            retry_count: retry.map(|retry| retry.retry_count).unwrap_or(0),
            max_retries: retry.map(|retry| retry.max_retries).unwrap_or(2),
            last_error: retry.map(|retry| retry.last_error.clone()),
            allowed_actions: input.constraints.allowed_actions,
            default_action: default_action(event),
        };
        let retry_section = retry_section(&prompt_context);
        let actions = available_actions(&prompt_context);
        Ok(format!(
            "[System Role]\nYou are a strict decision engine.\nReturn valid JSON only.\n\n[Task]\n{}\n\n[Current Event]\n{}\n\n[Context]\n{}\n{}\n[Constraints]\n- Only use allowed actions\n- Do not invent fields\n- Output must match schema\n- No explanations\n- Preserve request_id exactly\n\nAvailable actions:\n{}\n\n[Output Schema]\n{}",
            prompt_context.task,
            prompt_context.event_summary,
            prompt_context.context,
            retry_section,
            actions,
            prompt_context.schema
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

fn event_summary(event: &ControlEvent) -> String {
    match &event.payload {
        ControlPayload::Decision {
            reason,
            context,
            options,
            default,
        } => format!(
            "decision_required\nReason: {reason}\nDefault action: {}\nAllowed actions: {}\nContext: {}",
            default.as_str(),
            options
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            serde_json::to_string(context).unwrap_or_else(|_| "{}".to_string())
        ),
        ControlPayload::Input {
            prompt,
            schema,
            required,
        } => format!(
            "input_required\nPrompt: {prompt}\nRequired: {required}\nInput schema: {}",
            serde_json::to_string(schema).unwrap_or_else(|_| "{}".to_string())
        ),
        ControlPayload::Approval {
            action,
            risk,
            diff,
            files,
        } => format!(
            "approval_required\nAction: {action}\nRisk: {}\nFiles: {}\nDiff summary: {}",
            risk.as_str(),
            files.join(", "),
            summarize_diff(diff)
        ),
    }
}

fn response_schema_json(
    event: &ControlEvent,
    allowed_actions: &[DecisionAction],
) -> Result<String, String> {
    let schema = match &event.payload {
        ControlPayload::Input { .. } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["response_to", "request_id", "data"],
            "properties": {
                "response_to": { "const": event.event.as_str() },
                "request_id": { "const": event.request_id },
                "data": {}
            }
        }),
        ControlPayload::Decision { .. } | ControlPayload::Approval { .. } => {
            serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["response_to", "request_id", "action"],
                "properties": {
                    "response_to": { "const": event.event.as_str() },
                    "request_id": { "const": event.request_id },
                    "action": {
                        "enum": allowed_actions
                            .iter()
                            .map(|action| action.as_str())
                            .collect::<Vec<_>>()
                    }
                }
            })
        }
    };
    serde_json::to_string_pretty(&schema).map_err(|err| err.to_string())
}

fn retry_section(context: &PromptContext) -> String {
    if context.retry_count == 0 {
        return String::new();
    }
    format!(
        "\n[Retry Section]\nPrevious attempt failed.\n\nReason:\n{}\n\nRetry attempt: {}/{}\n\nFix the issue and return valid JSON.\n\n",
        context.last_error.as_deref().unwrap_or("Unknown error"),
        context.retry_count,
        context.max_retries
    )
}

fn available_actions(context: &PromptContext) -> String {
    context
        .allowed_actions
        .iter()
        .map(|action| {
            if *action == context.default_action {
                format!("- {} (default)", action.as_str())
            } else {
                format!("- {}", action.as_str())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn default_action(event: &ControlEvent) -> DecisionAction {
    match &event.payload {
        ControlPayload::Decision { default, .. } => *default,
        ControlPayload::Approval { .. } | ControlPayload::Input { .. } => DecisionAction::Abort,
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
            retry_count: 1,
            max_retries: 2,
            last_error: "Invalid JSON format".to_string(),
        };

        let prompt = PromptBuilder.build_retry(&event, &session, &retry).unwrap();

        assert!(prompt.contains("You are a strict decision engine."));
        assert!(prompt.contains("Return valid JSON only."));
        assert!(prompt.contains("Previous attempt failed."));
        assert!(prompt.contains("Invalid JSON format"));
        assert!(prompt.contains("Retry attempt: 1/2"));
        assert!(prompt.contains("- abort (default)"));
        assert!(prompt.contains("\"additionalProperties\": false"));
    }
}
