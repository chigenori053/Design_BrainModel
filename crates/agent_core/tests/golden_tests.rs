use core_types::ObjectiveVector;
use std::fs;
use std::path::PathBuf;

use agent_core::capability::{
    LinearObjectiveScorer, ScoringCapability, SearchHit, execute_soft_search_core,
    rank_hits_with_scorer,
};
use agent_core::domain::{Hypothesis, Score};
use agent_core::runtime::execute_soft_trace;

struct LenScorer;

impl ScoringCapability for LenScorer {
    fn score(&self, hypothesis: &Hypothesis) -> Score {
        Score(hypothesis.content.len() as f64)
    }
}

#[test]
fn golden_scalar_score_matches_legacy_formula() {
    let obj = ObjectiveVector {
        f_struct: 0.77,
        f_field: 0.11,
        f_risk: 0.23,
        f_shape: 0.91,
    };
    let legacy = agent_core::scalar_score(&obj);
    let via_capability = LinearObjectiveScorer.score_objective(&obj);
    assert!((legacy - via_capability).abs() < 1e-12);
}

#[test]
fn golden_search_ranking_matches_legacy_implementation() {
    let hits = vec![
        SearchHit {
            title: "A".to_string(),
            snippet: "short".to_string(),
        },
        SearchHit {
            title: "B".to_string(),
            snippet: "much longer snippet".to_string(),
        },
        SearchHit {
            title: "C".to_string(),
            snippet: "mid".to_string(),
        },
    ];
    let scorer = LenScorer;

    let legacy = legacy_rank_hits(&hits, &scorer)
        .into_iter()
        .map(|(hit, score)| format!("{score:.3}: {}", hit.title))
        .collect::<Vec<_>>();

    let migrated = rank_hits_with_scorer(&hits, &scorer)
        .into_iter()
        .map(|(hit, score)| format!("{score:.3}: {}", hit.title))
        .collect::<Vec<_>>();

    assert_eq!(legacy, migrated);
}

fn legacy_rank_hits<S: ScoringCapability>(hits: &[SearchHit], scorer: &S) -> Vec<(SearchHit, f64)> {
    let mut scored = Vec::with_capacity(hits.len());
    for (idx, hit) in hits.iter().enumerate() {
        let h = Hypothesis {
            id: format!("hypo-{idx}"),
            content: format!("{}: {}", hit.title, hit.snippet),
        };
        let Score(score) = scorer.score(&h);
        scored.push((hit.clone(), score));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

#[test]
fn golden_soft_trace_runtime_matches_core_and_writes_raw_objectives() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("tmp_soft_trace_raw.csv");
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("failed to create target dir");
    }
    let _ = fs::remove_file(&out);

    let config = agent_core::TraceRunConfig {
        depth: 3,
        beam: 4,
        seed: 42,
        norm_alpha: 0.1,
        adaptive_alpha: false,
        raw_output_path: Some(out.clone()),
    };
    let params = agent_core::SoftTraceParams::default();

    let core = execute_soft_search_core(config.clone(), params);
    let runtime_trace = execute_soft_trace(config, params);

    assert_eq!(runtime_trace.len(), core.trace.len());
    let lhs = runtime_trace.iter().map(trace_signature).collect::<Vec<_>>();
    let rhs = core.trace.iter().map(trace_signature).collect::<Vec<_>>();
    assert_eq!(lhs, rhs);
    assert!(out.exists(), "runtime should materialize raw objective file");
    let csv = fs::read_to_string(&out).expect("failed to read raw objective file");
    assert!(csv.contains("depth,candidate_id,objective_0"));
}

fn trace_signature(row: &agent_core::TraceRow) -> (usize, f32, f32, usize, usize, bool, usize, usize) {
    (
        row.depth,
        row.lambda,
        row.delta_lambda,
        row.pareto_size,
        row.pareto_front_size_per_depth,
        row.collapse_flag,
        row.distance_calls,
        row.nn_distance_calls,
    )
}
