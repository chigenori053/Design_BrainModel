use crate::model::{DecisionDto, DecisionSummaryDto, ConsensusStatus, ConfidenceLevel, EntropyLevel};
use crate::event::UiEvent;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use log::{info, error};

pub struct HybridVmClient {
    base_url: String,
    client: Client,
}

#[derive(Deserialize)]
struct RawDecisionDto {
    id: String,
    status: String,
    selected_candidate: Option<String>,
    evaluator_count: usize,
    confidence: String,
    entropy: String,
    explanation: String,
    human_override: bool,
}

#[derive(Deserialize)]
struct RawDecisionSummaryDto {
    id: String,
    status: String,
    is_reevaluation: bool,
}

#[derive(Serialize)]
struct EventRequest {
    #[serde(rename = "type")]
    event_type: String,
    payload: serde_json::Value,
}

impl HybridVmClient {
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:8000".to_string(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(1))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn fetch_latest_decision(&self) -> DecisionDto {
        let url = format!("{}/decision/latest", self.base_url);
        match self.client.get(&url).send() {
            Ok(resp) => {
                if let Ok(raw) = resp.json::<RawDecisionDto>() {
                    return DecisionDto {
                        id: raw.id,
                        status: map_status(&raw.status),
                        selected_candidate: raw.selected_candidate,
                        evaluator_count: raw.evaluator_count,
                        confidence: map_confidence(&raw.confidence),
                        entropy: map_entropy(&raw.entropy),
                        explanation: raw.explanation,
                        human_override: raw.human_override,
                    };
                }
            }
            Err(e) => {
                error!("Failed to fetch decision: {}", e);
            }
        }

        // Default / Error State
        DecisionDto {
            id: "conn-err".to_string(),
            status: ConsensusStatus::Pending,
            selected_candidate: None,
            evaluator_count: 0,
            confidence: ConfidenceLevel::Low,
            entropy: EntropyLevel::High,
            explanation: "Connecting to HybridVM...".to_string(),
            human_override: false,
        }
    }

    pub fn fetch_history(&self) -> Vec<DecisionSummaryDto> {
        let url = format!("{}/decision/history", self.base_url);
        match self.client.get(&url).send() {
            Ok(resp) => {
                 if let Ok(raw_list) = resp.json::<Vec<RawDecisionSummaryDto>>() {
                     return raw_list.into_iter().map(|raw| DecisionSummaryDto {
                         id: raw.id,
                         status: map_status(&raw.status),
                         is_reevaluation: raw.is_reevaluation,
                     }).collect();
                 }
            },
            Err(_) => {}
        }
        vec![]
    }

    pub fn send_event(&self, event: UiEvent) {
        let url = format!("{}/event", self.base_url);
        
        let (type_str, payload) = match &event {
            UiEvent::UserInput(text) => (
                "USER_INPUT", 
                serde_json::json!({ "text": text })
            ),
            UiEvent::RequestReevaluation => (
                "REQUEST_REEVALUATION",
                serde_json::json!({})
            ),
            UiEvent::HumanOverride { decision, reason } => (
                "HUMAN_OVERRIDE",
                serde_json::json!({ "decision": format!("{:?}", decision), "reason": reason })
            ),
        };
        
        let req = EventRequest {
            event_type: type_str.to_string(),
            payload,
        };
        
        info!("Sending Event: {:?}", req.event_type);
        if let Err(e) = self.client.post(&url).json(&req).send() {
            error!("Failed to send event: {}", e);
        }
    }
}

fn map_status(s: &str) -> ConsensusStatus {
    match s {
        "ACCEPT" => ConsensusStatus::Reached,
        "REVIEW" => ConsensusStatus::Reevaluating,
        "REJECT" => ConsensusStatus::Failed,
        "WAITING" => ConsensusStatus::Pending,
        _ => ConsensusStatus::Pending,
    }
}

fn map_confidence(s: &str) -> ConfidenceLevel {
    match s {
        "HIGH" => ConfidenceLevel::High,
        "MEDIUM" => ConfidenceLevel::Medium,
        "LOW" => ConfidenceLevel::Low,
        _ => ConfidenceLevel::Low,
    }
}

fn map_entropy(s: &str) -> EntropyLevel {
    match s {
        "HIGH" => EntropyLevel::High,
        "MEDIUM" => EntropyLevel::Medium,
        "LOW" => EntropyLevel::Low,
        _ => EntropyLevel::High,
    }
}
