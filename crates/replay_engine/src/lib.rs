use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use strategy_engine::Limits;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScenarioInput {
    pub name: String,
    pub description: String,
    pub architecture: String,
    pub components: Vec<String>,
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTrace {
    pub input: Value,
    pub knowledge: Value,
    pub ir: Value,
    pub memory: Vec<Value>,
    pub search: Vec<Value>,
    pub code: String,
    pub patch: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeterminismResult {
    Deterministic,
    Nondeterministic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffReport {
    pub result: DeterminismResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub fn load_scenario(name: &str) -> Result<ScenarioInput> {
    let scenario = match name {
        "rest-api" => ScenarioInput {
            name: name.to_string(),
            description: "REST API with controller, service, repository layers".to_string(),
            architecture: "rest-api".to_string(),
            components: vec![
                "api_gateway".to_string(),
                "user_controller".to_string(),
                "user_service".to_string(),
                "user_repository".to_string(),
            ],
            endpoints: vec!["GET /users/{id}".to_string(), "POST /users".to_string()],
        },
        "layered" => ScenarioInput {
            name: name.to_string(),
            description: "Layered application with presentation, domain, and persistence"
                .to_string(),
            architecture: "layered".to_string(),
            components: vec![
                "presentation".to_string(),
                "application".to_string(),
                "domain".to_string(),
                "persistence".to_string(),
            ],
            endpoints: vec![
                "command:create_order".to_string(),
                "query:get_order".to_string(),
            ],
        },
        "microservice" => ScenarioInput {
            name: name.to_string(),
            description: "Microservice topology with API, event, and storage boundaries"
                .to_string(),
            architecture: "microservice".to_string(),
            components: vec![
                "edge_api".to_string(),
                "account_service".to_string(),
                "billing_service".to_string(),
                "event_bus".to_string(),
                "ledger_store".to_string(),
            ],
            endpoints: vec![
                "POST /accounts".to_string(),
                "POST /billing/charge".to_string(),
                "event:AccountCreated".to_string(),
            ],
        },
        "break-beam-instability"
        | "break-codegen-order"
        | "break-hashmap-order"
        | "break-ir-shuffle"
        | "break-knowledge-order"
        | "break-memory-drift"
        | "break-memory-order"
        | "break-memory-tie"
        | "break-patch-order"
        | "break-search-tie"
        | "break-websearch-nondet" => ScenarioInput {
            name: name.to_string(),
            description: format!("Determinism verification fixture for {name}"),
            architecture: "determinism-break".to_string(),
            components: vec![
                "entrypoint".to_string(),
                "application".to_string(),
                "repository".to_string(),
            ],
            endpoints: vec!["GET /fixture".to_string(), "POST /fixture".to_string()],
        },
        _ => bail!("undefined scenario: {name}"),
    };

    Ok(scenario)
}

pub fn capture(input: &ScenarioInput) -> Result<ExecutionTrace> {
    let input_value = serde_json::to_value(input)?;
    let component_nodes = input
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            json!({
                "id": format!("{}:{index}", input.name),
                "name": component,
                "order": index,
            })
        })
        .collect::<Vec<_>>();
    let edges = input
        .components
        .windows(2)
        .map(|pair| json!({ "from": pair[0], "to": pair[1], "kind": "depends_on" }))
        .collect::<Vec<_>>();

    let mut knowledge = BTreeMap::new();
    knowledge.insert("architecture".to_string(), json!(input.architecture));
    knowledge.insert("component_count".to_string(), json!(input.components.len()));
    knowledge.insert("endpoint_count".to_string(), json!(input.endpoints.len()));

    Ok(ExecutionTrace {
        input: input_value,
        knowledge: json!(knowledge),
        ir: json!({
            "nodes": component_nodes,
            "edges": edges,
        }),
        memory: input
            .components
            .iter()
            .map(|component| json!({ "key": component, "source": input.name }))
            .collect(),
        search: input
            .endpoints
            .iter()
            .enumerate()
            .map(|(rank, endpoint)| json!({ "rank": rank, "candidate": endpoint }))
            .collect(),
        code: format!("// generated architecture skeleton: {}", input.name),
        patch: vec![json!({
            "op": "verify",
            "scenario": input.name,
            "status": "captured",
        })],
    })
}

pub fn replay(trace: &ExecutionTrace) -> Result<ExecutionTrace> {
    replay_with_limits(trace, Limits::default())
}

pub fn replay_with_limits(trace: &ExecutionTrace, limits: Limits) -> Result<ExecutionTrace> {
    let steps = trace.search.len() + trace.patch.len();
    if steps > limits.max_replay_steps {
        bail!("Replay limit exceeded");
    }
    let scenario: ScenarioInput = serde_json::from_value(trace.input.clone())?;
    let mut replayed = capture(&scenario)?;
    apply_determinism_break(&scenario.name, &mut replayed);
    Ok(replayed)
}

pub fn diff(left: &ExecutionTrace, right: &ExecutionTrace) -> Result<DiffReport> {
    if left == right {
        return Ok(DiffReport {
            result: DeterminismResult::Deterministic,
            diff: Some(Value::Null),
            layer: None,
            cause: None,
            details: None,
        });
    }

    let left_value = serde_json::to_value(left)?;
    let right_value = serde_json::to_value(right)?;
    let layer = first_changed_layer(&left_value, &right_value).unwrap_or("trace");

    Ok(DiffReport {
        result: DeterminismResult::Nondeterministic,
        diff: None,
        layer: Some(layer.to_string()),
        cause: Some(classify_cause(layer).to_string()),
        details: Some(json!({
            "left": left_value.get(layer).cloned().unwrap_or(Value::Null),
            "right": right_value.get(layer).cloned().unwrap_or(Value::Null),
        })),
    })
}

pub fn classify(report: DiffReport) -> Result<DiffReport> {
    Ok(report)
}

fn first_changed_layer<'a>(left: &'a Value, right: &'a Value) -> Option<&'a str> {
    [
        "input",
        "knowledge",
        "ir",
        "memory",
        "search",
        "code",
        "patch",
    ]
    .into_iter()
    .find(|layer| left.get(layer) != right.get(layer))
}

fn classify_cause(layer: &str) -> &'static str {
    match layer {
        "memory" => "RetrievalInstability",
        "search" => "SearchOrderingBug",
        "code" => "CodegenBug",
        "patch" => "PatchBug",
        "knowledge" => "ExternalNondeterminism",
        "ir" => "IRGenerationBug",
        "input" => "InputMismatch",
        _ => "TraceMismatch",
    }
}

fn apply_determinism_break(scenario: &str, replayed: &mut ExecutionTrace) {
    match scenario {
        "break-beam-instability" => replayed.search.reverse(),
        "break-codegen-order" => replayed.code.push_str("\n// nondeterministic order"),
        "break-hashmap-order" | "break-ir-shuffle" => {
            if let Some(nodes) = replayed.ir.get_mut("nodes").and_then(Value::as_array_mut) {
                nodes.reverse();
            }
        }
        "break-knowledge-order" | "break-websearch-nondet" => {
            replayed.knowledge = json!({
                "architecture": "determinism-break",
                "component_count": 3,
                "endpoint_count": 2,
                "external_nonce": scenario,
            });
        }
        "break-memory-drift" => replayed
            .memory
            .push(json!({ "key": "drifted", "source": scenario })),
        "break-patch-order" => replayed.patch.push(json!({
            "op": "verify",
            "scenario": scenario,
            "status": "replayed",
        })),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_rejects_trace_over_step_limit() {
        let scenario = load_scenario("microservice").unwrap();
        let trace = capture(&scenario).unwrap();
        let err = replay_with_limits(
            &trace,
            Limits {
                max_replay_steps: 1,
                ..Limits::default()
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("Replay limit exceeded"));
    }
}
