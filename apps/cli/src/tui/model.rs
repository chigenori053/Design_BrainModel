use serde::{Deserialize, Serialize};

/// Full payload sent from runtime to UI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiPayload {
    pub trace: TraceViewModel,
    pub hypotheses: Vec<HypothesisViewModel>,
    pub memory: Vec<MemoryCandidateViewModel>,
    /// Currently selected hypothesis id.
    pub selected: Option<usize>,
}

// ── Trace ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceViewModel {
    pub request_id: String,
    pub steps: Vec<TraceStepViewModel>,
    pub stats: TraceStatsViewModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceStepViewModel {
    pub depth: usize,
    pub beam_width: usize,
    pub candidates: usize,
    pub pruned: usize,
    pub recall_hits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceStatsViewModel {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub recall_hit_rate: f32,
    pub avg_branching: f32,
}

// ── Hypothesis ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HypothesisViewModel {
    pub id: usize,
    pub parent: Option<usize>,
    pub depth: usize,
    pub score: f32,
    pub score_parts: ScorePartsViewModel,
    /// DAG cross-links (empty when parent relation is sufficient).
    pub relations: Vec<HypothesisRelationViewModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HypothesisRelationViewModel {
    pub to_id: usize,
    pub relation_type: String,
}

/// Score breakdown (all values 0.0–1.0).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScorePartsViewModel {
    pub relevance: f32,
    pub goal: f32,
    pub constraint: f32,
    pub memory: f32,
}

// ── Memory / Recall ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryCandidateViewModel {
    pub id: String,
    pub score: f32,
    pub source: String, // "cache" | "index" | "exact"
    pub rank: usize,
    pub tags: Vec<String>,
}

impl MemoryCandidateViewModel {
    /// Derive a human-readable source label from score heuristic.
    pub fn source_from_score(score: f32) -> &'static str {
        if score >= 0.90 {
            "exact"
        } else if score >= 0.75 {
            "cache"
        } else {
            "index"
        }
    }
}
