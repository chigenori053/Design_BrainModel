use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::control_event::{
    ControlEvent, ControlOutcome, ControlPayload, ControlResponse, DecisionAction, DecisionSource,
    RequestId, timestamp_now,
};
use crate::control_executor::{RunLogEntry, RunLogger, SafetySnapshot};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunDsl {
    version: String,
    task: String,
    pipeline: Vec<RunDslStep>,
    context: Option<RunDslContext>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunDslContext {
    file: Option<String>,
    code: Option<String>,
    validation_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunDslStep {
    #[serde(rename = "type")]
    step_type: RunDslStepType,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RunDslStepType {
    GeneratePatch,
    Validate,
    Apply,
}

#[derive(Debug, Clone)]
struct RunDslPlan {
    run_id: String,
    task: String,
    context: Option<RunDslContext>,
    steps: Vec<PlanStep>,
}

#[derive(Debug, Clone)]
struct PlanStep {
    id: String,
    step_type: RunDslStepType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DslErrorKind {
    JsonParse,
    SchemaValidation,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DslResponseError {
    kind: DslErrorKind,
    message: String,
}

impl DslResponseError {
    fn new(kind: DslErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl DslErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::JsonParse => "JsonParse",
            Self::SchemaValidation => "SchemaValidation",
            Self::Semantic => "Semantic",
        }
    }

    fn is_protocol(self) -> bool {
        matches!(self, Self::JsonParse | Self::SchemaValidation)
    }
}

pub fn handle_run_dsl(input: PathBuf) -> Result<(), String> {
    let canonical_input = input.canonicalize().map_err(|err| {
        format!(
            "ValidationError: failed to resolve {}: {err}",
            input.display()
        )
    })?;
    let dsl = load_and_validate(&canonical_input)?;
    let plan = build_plan(&canonical_input, &dsl)?;
    execute_plan(
        &std::env::current_dir().map_err(|err| err.to_string())?,
        &plan,
    )
}

pub fn handle_replay(run_id: &str) -> Result<(), String> {
    let workspace_root = std::env::current_dir().map_err(|err| err.to_string())?;
    let path = workspace_root
        .join(".dbm")
        .join("runs")
        .join(format!("{run_id}.jsonl"));
    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "ValidationError: failed to read replay log {}: {err}",
            path.display()
        )
    })?;

    println!("run_started");
    let mut failed = false;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let entry: RunLogEntry = serde_json::from_str(line)
            .map_err(|err| format!("ValidationError: failed to parse replay log: {err}"))?;
        match entry {
            RunLogEntry::Event { event, .. } => {
                let event: ControlEvent = serde_json::from_value(event).map_err(|err| {
                    format!("ValidationError: invalid control event in replay: {err}")
                })?;
                println!("step_started {}", event.step_id);
                println!("{}", event.event.as_str());
            }
            RunLogEntry::RetryAttempt { .. } => println!("retry"),
            RunLogEntry::FallbackTriggered { .. } => println!("fallback"),
            RunLogEntry::Decision { outcome, .. } if outcome.is_abort() => {
                failed = true;
            }
            RunLogEntry::RunFailed { .. } => {
                failed = true;
            }
            _ => {}
        }
    }
    if failed {
        println!("run_failed");
        return Err("run-dsl replay resolved to failure".to_string());
    }
    println!("run_completed");
    Ok(())
}

fn load_and_validate(input: &Path) -> Result<RunDsl, String> {
    let raw = fs::read_to_string(input)
        .map_err(|err| format!("ValidationError: failed to read {}: {err}", input.display()))?;
    let dsl: RunDsl =
        serde_json::from_str(&raw).map_err(|err| format!("ValidationError: {err}"))?;
    if dsl.version != "1.0" {
        return Err(format!(
            "ValidationError: unsupported version {}, expected 1.0",
            dsl.version
        ));
    }
    if dsl.task.trim().is_empty() {
        return Err("ValidationError: task must not be empty".to_string());
    }
    if dsl.pipeline.is_empty() {
        return Err("ValidationError: pipeline must not be empty".to_string());
    }
    if let Some(context) = &dsl.context {
        validate_context(context)?;
    }
    Ok(dsl)
}

fn validate_context(context: &RunDslContext) -> Result<(), String> {
    if context.file.is_none() && context.code.is_none() {
        return Err("ValidationError: context requires file or code".to_string());
    }
    if context.validation_error.is_some() && context.code.is_none() {
        return Err("ValidationError: validation_error requires code".to_string());
    }
    Ok(())
}

fn build_plan(input: &Path, dsl: &RunDsl) -> Result<RunDslPlan, String> {
    let raw = fs::read(input)
        .map_err(|err| format!("ValidationError: failed to read {}: {err}", input.display()))?;
    let run_id = deterministic_run_id(&raw);
    let steps = dsl
        .pipeline
        .iter()
        .enumerate()
        .map(|(idx, step)| PlanStep {
            id: format!("step-{:03}-{}", idx + 1, step.step_type.as_str()),
            step_type: step.step_type,
        })
        .collect();
    Ok(RunDslPlan {
        run_id,
        task: dsl.task.clone(),
        context: dsl.context.clone(),
        steps,
    })
}

fn execute_plan(workspace_root: &Path, plan: &RunDslPlan) -> Result<(), String> {
    reset_run_log(workspace_root, &plan.run_id)?;
    let logger = RunLogger::new(workspace_root, &plan.run_id).map_err(|err| err.to_string())?;
    println!("run_started");

    for step in &plan.steps {
        println!("step_started {}", step.id);
        let event = build_control_event(plan, step);
        logger
            .append(&RunLogEntry::Event {
                event: serde_json::to_value(&event).map_err(|err| err.to_string())?,
                timestamp: timestamp_now(),
            })
            .map_err(|err| err.to_string())?;
        logger
            .append(&RunLogEntry::SafetySnapshot {
                safety: SafetySnapshot {
                    event: "safety_snapshot".to_string(),
                    run_id: plan.run_id.clone(),
                    step_id: step.id.clone(),
                    attempts: 1,
                    max_attempts: 3,
                    loop_count: 1,
                    max_loops: plan.steps.len() as u8,
                },
                timestamp: timestamp_now(),
            })
            .map_err(|err| err.to_string())?;
        println!("{}", event.event.as_str());

        let outcome = run_agent_loop(&logger, &event, step.step_type)?;
        logger
            .append(&RunLogEntry::Decision {
                request_id: event.request_id,
                step_id: event.step_id.clone(),
                outcome: outcome.clone(),
                timestamp: timestamp_now(),
            })
            .map_err(|err| err.to_string())?;

        if outcome.is_abort() {
            logger
                .append(&RunLogEntry::RunFailed {
                    event: "run_failed".to_string(),
                    run_id: plan.run_id.clone(),
                    step_id: Some(step.id.clone()),
                    reason: "agent selected abort".to_string(),
                    timestamp: timestamp_now(),
                })
                .map_err(|err| err.to_string())?;
            println!("run_failed");
            return Err("run-dsl failed: agent selected abort".to_string());
        }
    }

    println!("run_completed");
    Ok(())
}

fn run_agent_loop(
    logger: &RunLogger,
    event: &ControlEvent,
    step_type: RunDslStepType,
) -> Result<ControlOutcome, String> {
    let responses = deterministic_agent_responses(event, step_type);
    let mut last_raw = None::<String>;
    let mut last_error = None::<String>;
    let validation_error = validation_error_from_event(event);

    if let Some(validation_error) = validation_error.as_deref() {
        logger
            .append(&RunLogEntry::ValidationError {
                run_id: event.run_id.clone(),
                step_id: event.step_id.clone(),
                request_id: event.request_id,
                error: validation_error.to_string(),
            })
            .map_err(|err| err.to_string())?;
    }

    for (attempt, raw) in responses.iter().enumerate() {
        if attempt > 0 {
            if let Some(validation_error) = validation_error.as_deref() {
                logger
                    .append(&RunLogEntry::RetryReason {
                        run_id: event.run_id.clone(),
                        step_id: event.step_id.clone(),
                        request_id: event.request_id,
                        attempt: attempt as u8,
                        reason: validation_error.to_string(),
                    })
                    .map_err(|err| err.to_string())?;
                logger
                    .append(&RunLogEntry::FixAttempt {
                        run_id: event.run_id.clone(),
                        step_id: event.step_id.clone(),
                        request_id: event.request_id,
                        attempt: attempt as u8,
                        validation_error: validation_error.to_string(),
                        strategy: fix_strategy(validation_error).to_string(),
                    })
                    .map_err(|err| err.to_string())?;
            }
        }
        logger
            .append(&RunLogEntry::AgentPrompt {
                run_id: event.run_id.clone(),
                step_id: event.step_id.clone(),
                request_id: event.request_id,
                attempt: attempt as u8,
                prompt: build_agent_prompt(event),
            })
            .map_err(|err| err.to_string())?;
        logger
            .append(&RunLogEntry::AgentResponseRaw {
                run_id: event.run_id.clone(),
                step_id: event.step_id.clone(),
                request_id: event.request_id,
                attempt: attempt as u8,
                raw: raw.clone(),
            })
            .map_err(|err| err.to_string())?;
        last_raw = Some(raw.clone());

        match parse_agent_response(event, raw) {
            Ok(response) => {
                logger
                    .append(&RunLogEntry::AgentResponseParsed {
                        run_id: event.run_id.clone(),
                        step_id: event.step_id.clone(),
                        request_id: event.request_id,
                        response: response.clone(),
                    })
                    .map_err(|err| err.to_string())?;
                logger
                    .append(&RunLogEntry::Response {
                        response: serde_json::to_value(response.clone())
                            .map_err(|err| err.to_string())?,
                        timestamp: timestamp_now(),
                    })
                    .map_err(|err| err.to_string())?;
                return response_to_outcome(event, &response);
            }
            Err(err) if attempt + 1 < responses.len() => {
                println!("retry");
                logger
                    .append(&RunLogEntry::RetryAttempt {
                        run_id: event.run_id.clone(),
                        step_id: event.step_id.clone(),
                        request_id: event.request_id,
                        attempt: attempt as u8,
                        error_kind: err.kind.as_str().to_string(),
                        error: err.message.clone(),
                    })
                    .map_err(|err| err.to_string())?;
                if err.kind.is_protocol() {
                    logger
                        .append(&RunLogEntry::ProtocolRetry {
                            run_id: event.run_id.clone(),
                            step_id: event.step_id.clone(),
                            request_id: event.request_id,
                            attempt: attempt as u8,
                            protocol_error_kind: err.kind.as_str().to_string(),
                            reason: err.message.clone(),
                        })
                        .map_err(|err| err.to_string())?;
                    logger
                        .append(&RunLogEntry::ProtocolFixAttempt {
                            run_id: event.run_id.clone(),
                            step_id: event.step_id.clone(),
                            request_id: event.request_id,
                            attempt: attempt.saturating_add(1) as u8,
                            required_fields: required_protocol_fields(event),
                        })
                        .map_err(|err| err.to_string())?;
                }
                last_error = Some(err.message);
            }
            Err(err) => {
                last_error = Some(err.message);
            }
        }
    }

    let response = default_response(event);
    println!("fallback");
    logger
        .append(&RunLogEntry::FallbackTriggered {
            run_id: event.run_id.clone(),
            step_id: event.step_id.clone(),
            request_id: event.request_id,
            last_raw,
            last_error_kind: Some("validation_error".to_string()),
            last_error,
            response: response.clone(),
        })
        .map_err(|err| err.to_string())?;
    logger
        .append(&RunLogEntry::Response {
            response: serde_json::to_value(response.clone()).map_err(|err| err.to_string())?,
            timestamp: timestamp_now(),
        })
        .map_err(|err| err.to_string())?;
    response_to_outcome(event, &response)
}

fn build_control_event(plan: &RunDslPlan, step: &PlanStep) -> ControlEvent {
    let request_id = deterministic_request_id(&plan.run_id, &step.id);
    let context = serde_json::json!({
        "task": plan.task,
        "step": step.step_type.as_str(),
        "context": plan.context.as_ref().map(context_json).unwrap_or(serde_json::Value::Null),
    });
    match step.step_type {
        RunDslStepType::GeneratePatch => ControlEvent::decision_required(
            &plan.run_id,
            &step.id,
            request_id,
            "generate_patch",
            context,
            vec![
                DecisionAction::Modify,
                DecisionAction::Retry,
                DecisionAction::Abort,
            ],
            DecisionAction::Modify,
        ),
        RunDslStepType::Validate => ControlEvent::decision_required(
            &plan.run_id,
            &step.id,
            request_id,
            "validate",
            context,
            vec![
                DecisionAction::Modify,
                DecisionAction::Retry,
                DecisionAction::Abort,
            ],
            DecisionAction::Modify,
        ),
        RunDslStepType::Apply => ControlEvent::approval_required(
            &plan.run_id,
            &step.id,
            request_id,
            "apply_patch",
            crate::control_event::RiskLevel::Low,
            "",
            Vec::new(),
        ),
    }
}

fn build_agent_prompt(event: &ControlEvent) -> String {
    let (task, context) = match &event.payload {
        ControlPayload::Decision { context, .. } => {
            let task = context
                .get("task")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let context_section = context
                .get("context")
                .filter(|value| !value.is_null())
                .map(build_context_section)
                .unwrap_or_default();
            (task, context_section)
        }
        ControlPayload::Approval { .. } | ControlPayload::Input { .. } => {
            (String::new(), String::new())
        }
    };
    format!(
        "Task:\n{}\n\nContext:\n{}\nCurrent Event:\n{}",
        task,
        context,
        event.event.as_str()
    )
}

fn build_context_section(context: &serde_json::Value) -> String {
    let mut section = String::new();
    if let Some(file) = context.get("file").and_then(|value| value.as_str()) {
        section.push_str(&format!("File: {file}\n"));
    }
    if let Some(code) = context.get("code").and_then(|value| value.as_str()) {
        section.push_str(&format!("Code:\n{code}\n"));
    }
    if let Some(error) = context
        .get("validation_error")
        .and_then(|value| value.as_str())
    {
        section.push_str(&format!("Validation Error:\n{error}\n"));
    }
    section
}

fn context_json(context: &RunDslContext) -> serde_json::Value {
    serde_json::json!({
        "file": context.file,
        "code": context.code,
        "validation_error": context.validation_error,
    })
}

fn deterministic_agent_responses(event: &ControlEvent, step_type: RunDslStepType) -> Vec<String> {
    match step_type {
        RunDslStepType::GeneratePatch | RunDslStepType::Apply => {
            vec![agent_response(event, DecisionAction::Modify)]
        }
        RunDslStepType::Validate => match validation_error_from_event(event) {
            Some(error) if is_supported_validation_error(&error) => {
                vec![
                    "not-json".to_string(),
                    agent_response(event, DecisionAction::Modify),
                ]
            }
            Some(_) | None => vec![
                "not-json".to_string(),
                serde_json::json!({
                    "response_to": event.event,
                    "request_id": RequestId::from_u128(7),
                    "action": "modify"
                })
                .to_string(),
                "still-not-json".to_string(),
            ],
        },
    }
}

fn validation_error_from_event(event: &ControlEvent) -> Option<String> {
    let ControlPayload::Decision { context, .. } = &event.payload else {
        return None;
    };
    context
        .get("context")
        .and_then(|context| context.get("validation_error"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

fn is_supported_validation_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("missing semicolon")
        || normalized.contains("expected `;`")
        || normalized.contains("type mismatch")
        || normalized.contains("mismatched types")
}

fn fix_strategy(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("semicolon") || normalized.contains("expected `;`") {
        "insert_missing_semicolon"
    } else if normalized.contains("type mismatch") || normalized.contains("mismatched types") {
        "align_expression_type"
    } else {
        "fallback"
    }
}

fn parse_agent_response(
    event: &ControlEvent,
    raw: &str,
) -> Result<ControlResponse, DslResponseError> {
    if !raw.trim_start().starts_with('{') {
        return Err(DslResponseError::new(
            DslErrorKind::JsonParse,
            "agent output must be a JSON object",
        ));
    }
    let response: ControlResponse = serde_json::from_str(raw).map_err(|err| {
        DslResponseError::new(
            DslErrorKind::SchemaValidation,
            format!("schema validation failed: {err}"),
        )
    })?;
    if response.response_to != event.event {
        return Err(DslResponseError::new(
            DslErrorKind::SchemaValidation,
            format!(
                "response_to mismatch: expected {}, got {}",
                event.event.as_str(),
                response.response_to.as_str()
            ),
        ));
    }
    if response.request_id != event.request_id {
        return Err(DslResponseError::new(
            DslErrorKind::SchemaValidation,
            format!(
                "request_id mismatch: expected {}, got {}",
                event.request_id, response.request_id
            ),
        ));
    }
    if response.step_id != event.step_id {
        return Err(DslResponseError::new(
            DslErrorKind::SchemaValidation,
            format!(
                "step_id mismatch: expected {}, got {}",
                event.step_id, response.step_id
            ),
        ));
    }
    if response.run_id != event.run_id {
        return Err(DslResponseError::new(
            DslErrorKind::SchemaValidation,
            format!(
                "run_id mismatch: expected {}, got {}",
                event.run_id, response.run_id
            ),
        ));
    }
    match &event.payload {
        ControlPayload::Decision { options, .. } => {
            let action = response.action.ok_or_else(|| {
                DslResponseError::new(
                    DslErrorKind::SchemaValidation,
                    "decision response requires action",
                )
            })?;
            if !options.contains(&action) {
                return Err(DslResponseError::new(
                    DslErrorKind::Semantic,
                    format!("action {} is not allowed", action.as_str()),
                ));
            }
        }
        ControlPayload::Approval { .. } => {
            let action = response.action.ok_or_else(|| {
                DslResponseError::new(
                    DslErrorKind::SchemaValidation,
                    "approval response requires action",
                )
            })?;
            if !matches!(action, DecisionAction::Modify | DecisionAction::Abort) {
                return Err(DslResponseError::new(
                    DslErrorKind::Semantic,
                    format!("approval action {} is not allowed", action.as_str()),
                ));
            }
        }
        ControlPayload::Input { .. } => {}
    }
    Ok(response)
}

fn required_protocol_fields(event: &ControlEvent) -> Vec<String> {
    let mut fields = vec![
        "request_id".to_string(),
        "run_id".to_string(),
        "step_id".to_string(),
        "response_to".to_string(),
    ];
    if !matches!(event.payload, ControlPayload::Input { .. }) {
        fields.insert(0, "action".to_string());
    }
    fields
}

fn response_to_outcome(
    event: &ControlEvent,
    response: &ControlResponse,
) -> Result<ControlOutcome, String> {
    match event.payload {
        ControlPayload::Decision { .. } | ControlPayload::Approval { .. } => {
            Ok(ControlOutcome::Decision {
                action: response.action.unwrap_or(DecisionAction::Abort),
                source: DecisionSource::User,
            })
        }
        ControlPayload::Input { .. } => Ok(ControlOutcome::Input {
            data: response.data.clone().unwrap_or(serde_json::Value::Null),
            source: DecisionSource::User,
        }),
    }
}

fn default_response(event: &ControlEvent) -> ControlResponse {
    let action = match &event.payload {
        ControlPayload::Decision { default, .. } => Some(*default),
        ControlPayload::Approval { .. } => Some(DecisionAction::Abort),
        ControlPayload::Input { .. } => None,
    };
    ControlResponse {
        response_to: event.event,
        request_id: event.request_id,
        run_id: event.run_id.clone(),
        step_id: event.step_id.clone(),
        action,
        data: None,
    }
}

fn agent_response(event: &ControlEvent, action: DecisionAction) -> String {
    serde_json::json!({
        "response_to": event.event,
        "request_id": event.request_id,
        "run_id": event.run_id,
        "step_id": event.step_id,
        "action": action,
    })
    .to_string()
}

fn reset_run_log(workspace_root: &Path, run_id: &str) -> Result<(), String> {
    let dir = workspace_root.join(".dbm").join("runs");
    fs::create_dir_all(&dir).map_err(|err| format!("ValidationError: create runs dir: {err}"))?;
    fs::write(dir.join(format!("{run_id}.jsonl")), "")
        .map_err(|err| format!("ValidationError: reset run log: {err}"))
}

fn deterministic_run_id(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("run-dsl-{}", short_hex(&digest[..8]))
}

fn deterministic_request_id(run_id: &str, step_id: &str) -> Uuid {
    let digest = Sha256::digest(format!("{run_id}:{step_id}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

fn short_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl RunDslStepType {
    fn as_str(self) -> &'static str {
        match self {
            Self::GeneratePatch => "generate_patch",
            Self::Validate => "validate",
            Self::Apply => "apply",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_top_level_fields() {
        let raw = r#"{
            "version": "1.0",
            "task": "demo",
            "pipeline": [{"type": "generate_patch"}],
            "extra": true
        }"#;
        let err = serde_json::from_str::<RunDsl>(raw).unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn validates_required_semantics() {
        let dsl: RunDsl = serde_json::from_str(
            r#"{"version":"1.0","task":"demo","pipeline":[{"type":"validate"}]}"#,
        )
        .unwrap();
        assert_eq!(dsl.version, "1.0");
        assert_eq!(dsl.pipeline[0].step_type, RunDslStepType::Validate);
    }

    #[test]
    fn accepts_context_with_file_or_code() {
        let dsl: RunDsl = serde_json::from_str(
            r#"{
                "version":"1.0",
                "task":"demo",
                "context":{"file":"src/main.rs","code":"fn main() {}","validation_error":"missing semicolon"},
                "pipeline":[{"type":"generate_patch"}]
            }"#,
        )
        .unwrap();

        validate_context(dsl.context.as_ref().unwrap()).unwrap();
    }

    #[test]
    fn rejects_empty_context() {
        let dsl: RunDsl = serde_json::from_str(
            r#"{
                "version":"1.0",
                "task":"demo",
                "context":{},
                "pipeline":[{"type":"generate_patch"}]
            }"#,
        )
        .unwrap();

        let err = validate_context(dsl.context.as_ref().unwrap()).unwrap_err();
        assert!(err.contains("context requires file or code"));
    }

    #[test]
    fn rejects_validation_error_without_code() {
        let dsl: RunDsl = serde_json::from_str(
            r#"{
                "version":"1.0",
                "task":"demo",
                "context":{"file":"src/main.rs","validation_error":"missing semicolon"},
                "pipeline":[{"type":"generate_patch"}]
            }"#,
        )
        .unwrap();

        let err = validate_context(dsl.context.as_ref().unwrap()).unwrap_err();
        assert!(err.contains("validation_error requires code"));
    }

    #[test]
    fn rejects_unknown_context_fields() {
        let err = serde_json::from_str::<RunDsl>(
            r#"{
                "version":"1.0",
                "task":"demo",
                "context":{"diff":"nope"},
                "pipeline":[{"type":"generate_patch"}]
            }"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown field"));
    }
}
