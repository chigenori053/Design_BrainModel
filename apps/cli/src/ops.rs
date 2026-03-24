/// Operational Phase — structured logging, KPI metrics, failure classification.
///
/// Log format (saved as JSON per run):
/// {
///   "request_id": "...",
///   "input": "...",
///   "result": { ... },
///   "trace": { "steps": [...] },
///   "trace_stats": { "total_nodes": 124, ... },
///   "metrics": { "latency_ms": 420, "success": true, ... },
///   "failure": null | { "failure_type": "search", "input": "...", "actual": "..." }
/// }
use std::path::Path;

use serde::Serialize;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RunLog {
    pub request_id: String,
    pub input: String,
    pub result: Option<RunResultSummary>,
    pub trace: Option<TraceLogView>,
    pub trace_stats: Option<TraceStatsLog>,
    pub metrics: RunMetrics,
    pub failure: Option<FailureLog>,
    pub anomalies: AnomalyFlags,
}

#[derive(Debug, Serialize)]
pub struct RunResultSummary {
    pub project_root: String,
    pub files: Vec<String>,
    pub candidate_count: usize,
    pub selected_score: f64,
}

#[derive(Debug, Serialize)]
pub struct TraceLogView {
    pub steps: Vec<TraceStepLog>,
}

#[derive(Debug, Serialize)]
pub struct TraceStepLog {
    pub depth: usize,
    pub beam_width: usize,
    pub candidates: usize,
    pub pruned: usize,
    pub recall_hits: usize,
}

#[derive(Debug, Serialize)]
pub struct TraceStatsLog {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub recall_hit_rate: f32,
    pub avg_branching: f32,
}

#[derive(Debug, Serialize)]
pub struct RunMetrics {
    pub latency_ms: u128,
    pub success: bool,
    pub nodes_explored: usize,
    pub beam_avg: f32,
    pub cache_hit: bool,
}

/// Failure classification per spec section 5.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureType {
    /// Memory has a record but recall didn't retrieve it.
    RecallFailure,
    /// Beam search didn't produce usable candidates.
    SearchFailure,
    /// Score is inconsistent or out of range.
    EvaluationError,
    /// Structurally valid but rejected by constraints.
    ValidationFailure,
    /// Runtime / system error.
    SystemError,
}

#[derive(Debug, Serialize)]
pub struct FailureLog {
    pub failure_type: FailureType,
    pub input: String,
    pub actual: String,
}

/// Anomaly flags per spec section 6.
#[derive(Debug, Serialize)]
pub struct AnomalyFlags {
    /// latency > 2x baseline (2000 ms).
    pub latency_spike: bool,
    /// nodes_explored unusually high (> 500).
    pub exploration_explosion: bool,
    /// recall_hit_rate < 0.4.
    pub recall_low: bool,
}

// ── Baselines ─────────────────────────────────────────────────────────────────

const BASELINE_LATENCY_MS: u128 = 1_000;
const EXPLORATION_EXPLOSION_THRESHOLD: usize = 500;
const RECALL_LOW_THRESHOLD: f32 = 0.4;

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct RunLogBuilder {
    pub request_id: String,
    pub input: String,
    pub latency_ms: u128,
}

impl RunLogBuilder {
    /// Build a success log from a completed RuntimeResult.
    pub fn success(self, result: &runtime_core::stable_v03::RuntimeResult) -> RunLog {
        let (trace_view, trace_stats) = extract_trace(result);
        let nodes_explored = result.trace.generated_hypotheses;
        let beam_avg = compute_beam_avg(&trace_view);
        let recall_hit_rate = trace_stats
            .as_ref()
            .map(|s| s.recall_hit_rate)
            .unwrap_or(0.0);

        let metrics = RunMetrics {
            latency_ms: self.latency_ms,
            success: true,
            nodes_explored,
            beam_avg,
            cache_hit: result.trace.recall_used,
        };

        let anomalies = AnomalyFlags {
            latency_spike: self.latency_ms > BASELINE_LATENCY_MS * 2,
            exploration_explosion: nodes_explored > EXPLORATION_EXPLOSION_THRESHOLD,
            recall_low: recall_hit_rate < RECALL_LOW_THRESHOLD,
        };

        RunLog {
            request_id: self.request_id,
            input: self.input,
            result: Some(RunResultSummary {
                project_root: result.project_layout.root_dir.clone(),
                files: result
                    .project_layout
                    .files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
                candidate_count: result.trace.candidate_count,
                selected_score: result.trace.selected_score,
            }),
            trace: trace_view,
            trace_stats,
            metrics,
            failure: None,
            anomalies,
        }
    }

    /// Build a failure log.
    pub fn failure(self, failure_type: FailureType, actual: String) -> RunLog {
        let metrics = RunMetrics {
            latency_ms: self.latency_ms,
            success: false,
            nodes_explored: 0,
            beam_avg: 0.0,
            cache_hit: false,
        };
        let anomalies = AnomalyFlags {
            latency_spike: self.latency_ms > BASELINE_LATENCY_MS * 2,
            exploration_explosion: false,
            recall_low: true,
        };
        RunLog {
            request_id: self.request_id.clone(),
            input: self.input.clone(),
            result: None,
            trace: None,
            trace_stats: None,
            metrics,
            failure: Some(FailureLog {
                failure_type,
                input: self.input,
                actual,
            }),
            anomalies,
        }
    }
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/// Lightweight log for analyze operations.
#[derive(Debug, Serialize)]
pub struct AnalyzeLog {
    pub path: String,
    pub latency_ms: u128,
    pub success: bool,
    pub actual: Option<String>,
}

/// Write a RunLog as pretty JSON to `path`, creating parent directories if needed.
pub fn write_log(log: &RunLog, path: &Path) -> Result<(), String> {
    write_json(log, path)
}

/// Write an AnalyzeLog as pretty JSON.
pub fn write_analyze_log(log: &AnalyzeLog, path: &Path) -> Result<(), String> {
    write_json(log, path)
}

fn write_json<T: serde::Serialize>(value: &T, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create log dir {}: {e}", parent.display()))?;
        }
    }
    let json =
        serde_json::to_string_pretty(value).map_err(|e| format!("log serialization: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write log {}: {e}", path.display()))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_trace(
    result: &runtime_core::stable_v03::RuntimeResult,
) -> (Option<TraceLogView>, Option<TraceStatsLog>) {
    let Some(trace) = &result.reasoning_trace else {
        return (None, None);
    };
    let view = TraceLogView {
        steps: trace
            .steps
            .iter()
            .map(|s| TraceStepLog {
                depth: s.depth,
                beam_width: s.beam_width,
                candidates: s.candidates,
                pruned: s.pruned,
                recall_hits: s.recall_hits,
            })
            .collect(),
    };
    let stats = TraceStatsLog {
        total_nodes: trace.stats.total_nodes,
        max_depth: trace.stats.max_depth,
        recall_hit_rate: trace.stats.recall_hit_rate,
        avg_branching: trace.stats.avg_branching,
    };
    (Some(view), Some(stats))
}

fn compute_beam_avg(trace: &Option<TraceLogView>) -> f32 {
    let Some(t) = trace else { return 0.0 };
    if t.steps.is_empty() {
        return 0.0;
    }
    let sum: usize = t.steps.iter().map(|s| s.beam_width).sum();
    sum as f32 / t.steps.len() as f32
}
