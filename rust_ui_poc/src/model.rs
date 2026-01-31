use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Enums for ViewModels ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum L1ClusterStatus {
    #[default]
    Created,
    Active,
    Stale,
    Resolved,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionPolarityVm {
    Accept,
    #[default]
    Review,
    Reject,
}

// --- ViewModels (mirroring Python definitions) ---

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct L1AtomVm {
    pub id: String,
    pub r#type: String,
    pub content: String,
    pub source: String,
    pub timestamp: f64,
    pub referenced_in_l2_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct L1ClusterVm {
    pub id: String,
    pub status: L1ClusterStatus,
    pub l1_count: i64,
    pub entropy: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DecisionChipVm {
    pub l2_decision_id: String,
    pub head_generation_id: String,
    pub polarity: DecisionPolarityVm,
    pub scope: HashMap<String, serde_json::Value>,
    pub confidence: f64,
    pub entropy: f64,
}


// --- Command Payloads for Serialization ---

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateL1AtomPayload {
    pub content: String,
    pub r#type: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateL1ClusterPayload {
    pub l1_ids: Vec<String>,
}

// --- Main Application State ---

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub active_view: ActiveView,
    pub clusters: Vec<L1ClusterVm>,
    pub selected_cluster_index: Option<usize>,
    pub selected_cluster_atoms: Vec<L1AtomVm>,
    pub selected_decision: Option<DecisionChipVm>,
    pub logs: Vec<String>,
    pub input_buffer: String,
    pub is_running: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ActiveView {
    #[default]
    Clusters,
    Atoms,
    Decision,
    Logs,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            is_running: true,
            ..Self::default()
        }
    }

    pub fn quit(&mut self) {
        self.is_running = false;
    }
}
