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
    steps: Vec<PlanStep>,
}

#[derive(Debug, Clone)]
struct PlanStep {
    id: String,
    step_type: RunDslStepType,
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
    Ok(dsl)
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

    for (attempt, raw) in responses.iter().enumerate() {
        logger
            .append(&RunLogEntry::AgentPrompt {
                run_id: event.run_id.clone(),
                step_id: event.step_id.clone(),
                request_id: event.request_id,
                attempt: attempt as u8,
                prompt: format!("task_step={} event={}", event.step_id, event.event.as_str()),
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
                        error_kind: "validation_error".to_string(),
                        error: err.clone(),
                    })
                    .map_err(|err| err.to_string())?;
                last_error = Some(err);
            }
            Err(err) => {
                last_error = Some(err);
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
    match step.step_type {
        RunDslStepType::GeneratePatch => ControlEvent::decision_required(
            &plan.run_id,
            &step.id,
            request_id,
            "generate_patch",
            serde_json::json!({ "task": plan.task, "step": step.step_type.as_str() }),
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
            serde_json::json!({ "task": plan.task, "step": step.step_type.as_str() }),
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

fn deterministic_agent_responses(event: &ControlEvent, step_type: RunDslStepType) -> Vec<String> {
    match step_type {
        RunDslStepType::GeneratePatch | RunDslStepType::Apply => {
            vec![agent_response(event, DecisionAction::Modify)]
        }
        RunDslStepType::Validate => vec![
            "not-json".to_string(),
            serde_json::json!({
                "response_to": event.event,
                "request_id": RequestId::from_u128(7),
                "action": "modify"
            })
            .to_string(),
            "still-not-json".to_string(),
        ],
    }
}

fn parse_agent_response(event: &ControlEvent, raw: &str) -> Result<ControlResponse, String> {
    if !raw.trim_start().starts_with('{') {
        return Err("agent output must be a JSON object".to_string());
    }
    let response: ControlResponse =
        serde_json::from_str(raw).map_err(|err| format!("invalid agent response JSON: {err}"))?;
    if response.response_to != event.event {
        return Err(format!(
            "response_to mismatch: expected {}, got {}",
            event.event.as_str(),
            response.response_to.as_str()
        ));
    }
    if response.request_id != event.request_id {
        return Err(format!(
            "request_id mismatch: expected {}, got {}",
            event.request_id, response.request_id
        ));
    }
    if response.step_id != event.step_id {
        return Err(format!(
            "step_id mismatch: expected {}, got {}",
            event.step_id, response.step_id
        ));
    }
    match &event.payload {
        ControlPayload::Decision { options, .. } => {
            let action = response
                .action
                .ok_or_else(|| "decision response requires action".to_string())?;
            if !options.contains(&action) {
                return Err(format!("action {} is not allowed", action.as_str()));
            }
        }
        ControlPayload::Approval { .. } => {
            let action = response
                .action
                .ok_or_else(|| "approval response requires action".to_string())?;
            if !matches!(action, DecisionAction::Modify | DecisionAction::Abort) {
                return Err(format!(
                    "approval action {} is not allowed",
                    action.as_str()
                ));
            }
        }
        ControlPayload::Input { .. } => {}
    }
    Ok(response)
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
}
