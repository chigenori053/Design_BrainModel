use std::sync::Arc;

use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_engine::InMemoryEngine;
use runtime_core::CoreRuntime;

/// 共通の CoreRuntime を構築する
pub fn build_runtime() -> CoreRuntime {
    let memory = InMemoryEngine::default();
    let loaded = crate::memory_seed::load_default_seeds(&memory);
    if loaded > 0 {
        eprintln!("info: loaded {loaded} seed records into memory engine");
    }
    CoreRuntime::new_with_defaults(
        Arc::new(memory),
        Arc::new(DeterministicBeamSearchEngine::default()),
    )
}

/// Build a UiPayload from a completed RuntimeResult.
/// Exported so other CLI commands can call it (e.g. `generate --tui`).
pub fn ui_payload_from_result(
    result: &runtime_core::stable_v03::RuntimeResult,
) -> crate::tui::model::UiPayload {
    use crate::tui::model::{
        HypothesisViewModel, MemoryCandidateViewModel, ScorePartsViewModel, TraceStatsViewModel,
        TraceStepViewModel, TraceViewModel, UiPayload,
    };

    let (request_id, steps, stats) = result
        .reasoning_trace
        .as_ref()
        .map(|t| {
            let steps = t
                .steps
                .iter()
                .map(|s| TraceStepViewModel {
                    depth: s.depth,
                    beam_width: s.beam_width,
                    candidates: s.candidates,
                    pruned: s.pruned,
                    recall_hits: s.recall_hits,
                })
                .collect::<Vec<_>>();
            let stats = TraceStatsViewModel {
                total_nodes: t.stats.total_nodes,
                max_depth: t.stats.max_depth,
                recall_hit_rate: t.stats.recall_hit_rate,
                avg_branching: t.stats.avg_branching,
            };
            (t.request_id.0.clone(), steps, stats)
        })
        .unwrap_or_else(|| {
            (
                "unknown".to_string(),
                vec![],
                TraceStatsViewModel {
                    total_nodes: 0,
                    max_depth: 0,
                    recall_hit_rate: 0.0,
                    avg_branching: 0.0,
                },
            )
        });

    let hypotheses: Vec<HypothesisViewModel> = result
        .scored_candidates
        .iter()
        .enumerate()
        .map(|(idx, sc)| {
            let m = &sc.evaluation.metrics;
            HypothesisViewModel {
                id: idx,
                parent: if sc.candidate.depth > 0 {
                    Some(idx.saturating_sub(1))
                } else {
                    None
                },
                depth: sc.candidate.depth,
                score: sc.evaluation.score as f32,
                score_parts: ScorePartsViewModel {
                    relevance: m.modularity as f32,
                    goal: m.cohesion as f32,
                    constraint: (1.0 - m.coupling) as f32,
                    memory: (1.0 - m.complexity) as f32,
                },
                relations: vec![],
            }
        })
        .collect();

    let memory: Vec<MemoryCandidateViewModel> = result
        .recall_records
        .iter()
        .enumerate()
        .map(|(rank, r)| {
            let score = r.score as f32;
            MemoryCandidateViewModel {
                id: r.record.id.clone(),
                score,
                source: MemoryCandidateViewModel::source_from_score(score).to_string(),
                rank,
                tags: r.record.tags.iter().take(3).cloned().collect(),
            }
        })
        .collect();

    let selected = hypotheses.first().map(|h| h.id);

    UiPayload {
        trace: TraceViewModel {
            request_id,
            steps,
            stats,
        },
        hypotheses,
        memory,
        selected,
    }
}
