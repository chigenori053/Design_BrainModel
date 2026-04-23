/// Serializable types for the full pipeline trace (spec §6).
///
/// FullTrace freezes one complete execution run so it can be replayed exactly.
/// The JSON layout matches the spec's trace.json format.
use serde::{Deserialize, Serialize};

// ── Top-level trace ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FullTrace {
    pub input: InputSnapshot,
    pub knowledge: KnowledgeSnapshot,
    pub ir: IrSnapshot,
    pub memory: Vec<MemoryLayerEntry>,
    pub search: Vec<SearchLayerEntry>,
    pub code: String,
    pub patch: Vec<PatchEntry>,
    pub metadata: TraceMetadata,
}

// ── Input layer ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub state_id: u64,
    /// architecture_hash of the initial WorldState — key for determinism check.
    pub initial_state_hash: String,
    pub architecture: SerializedArchitecture,
    pub search_config: SerializedSearchConfig,
    pub score: f64,
    pub depth: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedArchitecture {
    pub classes: Vec<SerializedClass>,
    pub deps: Vec<SerializedDependency>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedClass {
    pub id: u64,
    pub name: String,
    pub structures: Vec<SerializedStructure>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedStructure {
    pub id: u64,
    pub name: String,
    pub units: Vec<SerializedUnit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedUnit {
    pub id: u64,
    pub name: String,
    pub layer: String,
    pub semantics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedDependency {
    pub from: u64,
    pub to: u64,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedSearchConfig {
    pub max_depth: usize,
    pub max_candidates: usize,
    pub beam_width: usize,
    pub experience_bias: f64,
    pub policy_bias: f64,
}

// ── Knowledge layer (WebSearch snapshot — spec §9.3 / §12) ───────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeSnapshot {
    /// Frozen external knowledge. On replay, these replace live WebSearch calls.
    pub documents: Vec<KnowledgeEntry>,
    pub source_count: usize,
    pub web_search_used: bool,
    /// Hash of all document content — used for layer diff.
    pub content_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub source: String,
    pub content: String,
    pub title: String,
    pub source_uri: String,
    pub reliability_hint: f64,
}

// ── IR layer ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IrSnapshot {
    pub module_count: usize,
    pub module_names: Vec<String>,
    pub dependency_count: usize,
    /// Hash of module_names joined — quick equality check.
    pub ir_hash: String,
}

impl IrSnapshot {
    pub fn empty() -> Self {
        Self {
            module_count: 0,
            module_names: vec![],
            dependency_count: 0,
            ir_hash: "0000000000000000".into(),
        }
    }
}

// ── Memory layer ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryLayerEntry {
    pub pattern_id: String,
    pub average_score: f64,
    pub frequency: usize,
    pub layer_sequence: Vec<String>,
    pub dependency_edge_count: usize,
}

// ── Search layer ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchLayerEntry {
    pub step_index: usize,
    pub branch_id: u64,
    pub state_hash: String,
    pub score: f64,
    pub depth: usize,
    pub pareto_rank: usize,
    pub source_action: Option<String>,
}

// ── Patch layer ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchEntry {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
}

// ── Metadata ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceMetadata {
    pub timestamp_utc: String,
    pub version: String,
    pub run_id: String,
    pub explored_state_count: usize,
    pub depth_best_scores: Vec<f64>,
}
