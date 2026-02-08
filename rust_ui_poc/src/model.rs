use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Enums for ViewModels ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesignDraftStatus {
    #[default]
    Draft,
    Review,
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
pub struct DesignDraftVm {
    pub id: String,
    pub title: String,
    pub description: String,
    pub source_l1_ids: Vec<String>,
    pub status: DesignDraftStatus,
    pub created_by: String, // "human" | "model"
    pub created_at: f64,
    pub feedback_text: String, // Natural language feedback
}

// --- PhaseC View Models ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GeometryPointVm {
    pub vector: Vec<f64>,
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvaluationReportVm {
    pub targets: Vec<String>,
    pub geometry_points: Vec<GeometryPointVm>,
    pub distances: Vec<Vec<f64>>,
    pub qualitative_notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AxisScoreVm {
    pub axis: String,
    pub score: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProposalDetailVm {
    pub id: String,
    pub title: String,
    pub target_type: String,
    pub abstract_structure: serde_json::Value,
    pub constraints: Vec<String>,
    pub axis_scores: Vec<AxisScoreVm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PhaseCState {
    pub report: EvaluationReportVm,
    pub proposals: Vec<ProposalDetailVm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HumanOverrideLogEntry {
    pub timestamp: u64,
    pub target_id: String,
    pub action: String,
    pub rationale: Option<String>,
}

// --- Command Payloads for Serialization ---

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateL1ClusterPayload {
    pub l1_ids: Vec<String>,
}

// --- Main Application State ---

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub active_view: ActiveView,
    pub logs: Vec<String>,
    pub input_buffer: String,
    pub input_mode: bool,
    pub free_notes: Vec<String>,
    pub l1_atoms: Vec<L1AtomVm>,
    pub l2_units: Vec<DesignDraftVm>,
    pub selected_l1_index: Option<usize>,
    pub selected_l2_index: Option<usize>,
    pub active_tab: ActiveTab,
    pub tab_messages: HashMap<ActiveTab, Vec<String>>,
    pub is_running: bool,
    pub phasec_state: Option<PhaseCState>,
    pub selected_proposal_index: usize,
    pub override_logs: Vec<HumanOverrideLogEntry>,
    pub override_input_mode: bool,
    pub override_buffer: String,
    pub show_help: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ActiveView {
    #[default]
    Normal,
    PhaseC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActiveTab {
    FreeNote,
    Understanding,
    DesignDraft,
}

impl Default for ActiveTab {
    fn default() -> Self {
        ActiveTab::FreeNote
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            is_running: true,
            input_mode: false,
            free_notes: Vec::new(),
            l1_atoms: Vec::new(),
            l2_units: Vec::new(),
            selected_l1_index: None,
            selected_l2_index: None,
            active_tab: ActiveTab::FreeNote,
            tab_messages: HashMap::new(),
            phasec_state: None,
            selected_proposal_index: 0,
            override_logs: Vec::new(),
            override_input_mode: false,
            override_buffer: String::new(),
            show_help: false,
            ..Self::default()
        }
    }

    pub fn quit(&mut self) {
        self.is_running = false;
    }
}
