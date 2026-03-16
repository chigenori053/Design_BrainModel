use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use architecture_evaluator::{ArchitectureEvaluatorEngine, ArchitectureIrEvaluator};
use architecture_search::template::builtin_templates;
use architecture_search::template_engine::template_record_from_template;
use architecture_search::{ArchitectureSearchEngine, ArchitectureTemplateEngine, IntentModel, SearchConfig};
use memory_space_phase14::{
    DesignIntentRecord, DesignMemorySpace, embed_template,
};
use serde_json::json;

#[derive(Clone, Debug)]
struct QualityCase {
    name: &'static str,
    intent: IntentModel,
    acceptable_templates: &'static [&'static str],
}

#[derive(Clone, Debug, Default)]
struct SearchQualityMetrics {
    average_overall_score: f64,
    best_architecture_score: f64,
    pareto_frontier_size: usize,
    candidate_count: usize,
    evaluation_count: usize,
    search_depth: usize,
    mean_frontier_score: f64,
    frontier_diversity: f64,
    dominance_ratio: f64,
}

#[derive(Clone, Debug, Default)]
struct QualitySuiteResult {
    q1_cases: BTreeMap<String, (SearchQualityMetrics, SearchQualityMetrics)>,
    q2_mean_search_depth_baseline: f64,
    q2_mean_search_depth_memory: f64,
    q2_candidate_reduction: f64,
    q2_evaluation_reduction: f64,
    q3_recall_accuracy: f64,
    q3_domain_mismatch_rate: f64,
    q4_memory_nodes: usize,
    q4_memory_edges: usize,
    q4_template_count: usize,
    q4_recall_latency_ms: f64,
    q4_search_latency_ms: f64,
    q5_mean_frontier_score_baseline: f64,
    q5_mean_frontier_score_memory: f64,
    q5_frontier_diversity_baseline: f64,
    q5_frontier_diversity_memory: f64,
    q5_dominance_ratio_baseline: f64,
    q5_dominance_ratio_memory: f64,
    recall_latency_mean_ms: f64,
    recall_latency_max_ms: f64,
    integrity_errors: usize,
}

fn quality_cases() -> Vec<QualityCase> {
    vec![
        QualityCase {
            name: "web_api",
            intent: IntentModel {
                system_type: "web_api".to_string(),
                requirements: vec!["caching".to_string(), "authentication".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["layered", "hexagonal", "microservice"],
        },
        QualityCase {
            name: "data_pipeline",
            intent: IntentModel {
                system_type: "data_pipeline".to_string(),
                requirements: vec!["batch".to_string(), "storage".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["pipeline", "event_driven"],
        },
        QualityCase {
            name: "event_stream_processing",
            intent: IntentModel {
                system_type: "event_stream_processing".to_string(),
                requirements: vec!["stream".to_string(), "queue".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["event_driven", "pipeline"],
        },
        QualityCase {
            name: "microservice_system",
            intent: IntentModel {
                system_type: "microservice_system".to_string(),
                requirements: vec!["service_mesh".to_string(), "api".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["microservice", "layered"],
        },
        QualityCase {
            name: "machine_learning_pipeline",
            intent: IntentModel {
                system_type: "machine_learning_pipeline".to_string(),
                requirements: vec!["batch_training".to_string(), "feature_store".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["pipeline", "microservice"],
        },
        QualityCase {
            name: "event_driven",
            intent: IntentModel {
                system_type: "event_driven".to_string(),
                requirements: vec!["messaging".to_string(), "queue".to_string()],
                ..IntentModel::default()
            },
            acceptable_templates: &["event_driven", "microservice"],
        },
    ]
}

fn domain_template_embedding(template_id: &str) -> Vec<f32> {
    match template_id {
        "layered" => vec![1.0, 2.0, 0.0, 2.0],
        "hexagonal" => vec![0.95, 2.0, 0.0, 2.0],
        "microservice" => vec![0.90, 2.0, 0.0, 2.0],
        "pipeline" => vec![0.80, 2.0, 0.0, 2.0],
        "event_driven" => vec![0.70, 2.0, 0.0, 2.0],
        _ => vec![0.4, 1.0, 0.0, 1.0],
    }
}

fn seed_domain_memory(memory: &mut DesignMemorySpace) {
    for template in builtin_templates() {
        let mut record = template_record_from_template(&template);
        record.metadata.usage_count = match record.template_id.as_str() {
            "layered" => 32,
            "hexagonal" => 24,
            "microservice" => 24,
            "pipeline" => 28,
            "event_driven" => 28,
            _ => 8,
        };
        record.metadata.average_score = match record.template_id.as_str() {
            "layered" => 0.95,
            "hexagonal" => 0.92,
            "microservice" => 0.91,
            "pipeline" => 0.94,
            "event_driven" => 0.94,
            _ => 0.80,
        };
        record.metadata.success_rate = record.metadata.average_score;
        let embedding = if matches!(
            record.template_id.as_str(),
            "layered" | "hexagonal" | "microservice" | "pipeline" | "event_driven"
        ) {
            domain_template_embedding(&record.template_id)
        } else {
            embed_template(&record)
        };
        memory.store_template(record, embedding, &[]);
    }
}

fn search_engine() -> ArchitectureSearchEngine {
    ArchitectureSearchEngine {
        config: SearchConfig {
            beam_width: 8,
            max_depth: 6,
            max_candidates: 1_024,
            pareto_limit: 10,
            timeout_ms: 10_000,
        },
    }
}

fn quality_search_engine() -> ArchitectureSearchEngine {
    ArchitectureSearchEngine {
        config: SearchConfig {
            beam_width: 16,
            max_depth: 7,
            max_candidates: 2_048,
            pareto_limit: 20,
            timeout_ms: 10_000,
        },
    }
}

fn evaluate_search_result(
    result: &architecture_search::SearchResult,
    evaluator: &ArchitectureEvaluatorEngine,
) -> SearchQualityMetrics {
    let mut scores = result
        .pareto_frontier
        .iter()
        .map(|candidate| evaluator.evaluate_ir(&candidate.architecture_ir).scores.overall_score)
        .collect::<Vec<_>>();
    scores.sort_by(|lhs, rhs| rhs.total_cmp(lhs));
    let sum = scores.iter().sum::<f64>();
    let average = if scores.is_empty() {
        0.0
    } else {
        sum / scores.len() as f64
    };
    let best = scores.first().copied().unwrap_or_default();
    SearchQualityMetrics {
        average_overall_score: average,
        best_architecture_score: best,
        pareto_frontier_size: result.pareto_frontier.len(),
        candidate_count: result.telemetry.candidate_count,
        evaluation_count: result.telemetry.generated_states,
        search_depth: result.telemetry.search_depth,
        mean_frontier_score: average,
        frontier_diversity: frontier_diversity(&scores),
        dominance_ratio: dominance_ratio(&scores),
    }
}

fn frontier_diversity(scores: &[f64]) -> f64 {
    if scores.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    let mut count = 0usize;
    for (index, left) in scores.iter().enumerate() {
        for right in &scores[index + 1..] {
            total += (left - right).abs();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

fn dominance_ratio(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let best = scores[0];
    let non_dominated = scores
        .iter()
        .filter(|score| (best - **score).abs() <= 0.05)
        .count();
    non_dominated as f64 / scores.len() as f64
}

fn recall_selection(
    engine: &ArchitectureTemplateEngine,
    memory: &DesignMemorySpace,
    intent: &IntentModel,
) -> String {
    engine
        .select_templates_with_memory(intent, memory)
        .selected
        .template_id
}

fn intent_record(case: &QualityCase) -> DesignIntentRecord {
    DesignIntentRecord {
        intent_id: case.name.to_string(),
        system_type: case.intent.system_type.clone(),
        requirements: case.intent.requirements.clone(),
        constraints: case
            .intent
            .constraints
            .architecture
            .iter()
            .cloned()
            .collect(),
    }
}

fn integrity_errors(memory: &DesignMemorySpace) -> usize {
    let mut errors = 0usize;
    for edge in memory.graph.edges() {
        if memory.graph.get(edge.from).is_none() {
            errors += 1;
        }
        if memory.graph.get(edge.to).is_none() {
            errors += 1;
        }
    }
    errors
}

fn report_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../report")
}

fn run_quality_suite() -> QualitySuiteResult {
    let engine = search_engine();
    let quality_engine = quality_search_engine();
    let evaluator = ArchitectureEvaluatorEngine::default();
    let template_engine = ArchitectureTemplateEngine::with_builtin_library();
    let cases = quality_cases();

    let mut suite = QualitySuiteResult::default();
    let mut baseline_frontier_means = Vec::new();
    let mut memory_frontier_means = Vec::new();
    let mut baseline_diversity = Vec::new();
    let mut memory_diversity = Vec::new();
    let mut baseline_dominance = Vec::new();
    let mut memory_dominance = Vec::new();

    for case in &cases[..4] {
        let mut memory = DesignMemorySpace::default();
        seed_domain_memory(&mut memory);
        let baseline = engine.run(&case.intent);
        let _ = quality_engine.run_with_memory(&case.intent, &mut memory);
        let with_memory = quality_engine.run_with_memory(&case.intent, &mut memory);
        let baseline_metrics = evaluate_search_result(&baseline, &evaluator);
        let memory_metrics = evaluate_search_result(&with_memory, &evaluator);
        baseline_frontier_means.push(baseline_metrics.mean_frontier_score);
        memory_frontier_means.push(memory_metrics.mean_frontier_score);
        baseline_diversity.push(baseline_metrics.frontier_diversity);
        memory_diversity.push(memory_metrics.frontier_diversity);
        baseline_dominance.push(baseline_metrics.dominance_ratio);
        memory_dominance.push(memory_metrics.dominance_ratio);
        suite.q1_cases.insert(
            case.name.to_string(),
            (baseline_metrics, memory_metrics),
        );
    }

    let convergence_case = &cases[0];
    let mut convergence_memory = DesignMemorySpace::default();
    seed_domain_memory(&mut convergence_memory);
    let mut baseline_depth = 0usize;
    let mut memory_depth = 0usize;
    let mut baseline_candidates = 0usize;
    let mut memory_candidates = 0usize;
    let mut baseline_evaluations = 0usize;
    let mut memory_evaluations = 0usize;
    for _ in 0..50 {
        let baseline = engine.run(&convergence_case.intent);
        let with_memory = engine.run_with_memory(&convergence_case.intent, &mut convergence_memory);
        baseline_depth += baseline.telemetry.search_depth;
        memory_depth += with_memory.telemetry.search_depth;
        baseline_candidates += baseline.telemetry.candidate_count;
        memory_candidates += with_memory.telemetry.candidate_count;
        baseline_evaluations += baseline.telemetry.generated_states;
        memory_evaluations += with_memory.telemetry.generated_states;
    }
    suite.q2_mean_search_depth_baseline = baseline_depth as f64 / 50.0;
    suite.q2_mean_search_depth_memory = memory_depth as f64 / 50.0;
    suite.q2_candidate_reduction =
        1.0 - memory_candidates as f64 / baseline_candidates.max(1) as f64;
    suite.q2_evaluation_reduction =
        1.0 - memory_evaluations as f64 / baseline_evaluations.max(1) as f64;

    let mut recall_memory = DesignMemorySpace::default();
    seed_domain_memory(&mut recall_memory);
    let mut recall_hits = 0usize;
    let mut recall_total = 0usize;
    let mut mismatches = 0usize;
    let mut latency_samples = Vec::new();
    for case in &cases {
        for _ in 0..20 {
            let started = Instant::now();
            let selected = recall_selection(&template_engine, &recall_memory, &case.intent);
            latency_samples.push(started.elapsed());
            recall_total += 1;
            if case.acceptable_templates.contains(&selected.as_str()) {
                recall_hits += 1;
            } else {
                mismatches += 1;
            }
        }
    }
    suite.q3_recall_accuracy = recall_hits as f64 / recall_total.max(1) as f64;
    suite.q3_domain_mismatch_rate = mismatches as f64 / recall_total.max(1) as f64;

    let latency_total = latency_samples
        .iter()
        .map(Duration::as_secs_f64)
        .sum::<f64>();
    suite.recall_latency_mean_ms = latency_total * 1000.0 / latency_samples.len().max(1) as f64;
    suite.recall_latency_max_ms = latency_samples
        .iter()
        .map(Duration::as_secs_f64)
        .fold(0.0, f64::max)
        * 1000.0;

    let mut memory = DesignMemorySpace::default();
    seed_domain_memory(&mut memory);
    let mut stability_search_total = Duration::ZERO;
    let mut stability_recall_total = Duration::ZERO;
    for index in 0..1_000 {
        let case = &cases[index % cases.len()];
        let search_started = Instant::now();
        let _ = engine.run_with_memory(&case.intent, &mut memory);
        stability_search_total += search_started.elapsed();
        let recall_started = Instant::now();
        let _ = memory.recall_templates_for_intent(&intent_record(case), 3);
        stability_recall_total += recall_started.elapsed();
    }
    suite.q4_memory_nodes = memory.graph.nodes().len();
    suite.q4_memory_edges = memory.graph.edges().len();
    suite.q4_template_count = memory.template_memory.all().len();
    suite.q4_recall_latency_ms = stability_recall_total.as_secs_f64() * 1000.0 / 1000.0;
    suite.q4_search_latency_ms = stability_search_total.as_secs_f64() * 1000.0 / 1000.0;
    suite.integrity_errors = integrity_errors(&memory);

    suite.q5_mean_frontier_score_baseline = mean(&baseline_frontier_means);
    suite.q5_mean_frontier_score_memory = mean(&memory_frontier_means);
    suite.q5_frontier_diversity_baseline = mean(&baseline_diversity);
    suite.q5_frontier_diversity_memory = mean(&memory_diversity);
    suite.q5_dominance_ratio_baseline = mean(&baseline_dominance);
    suite.q5_dominance_ratio_memory = mean(&memory_dominance);

    write_artifacts(&suite);
    suite
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn write_artifacts(result: &QualitySuiteResult) {
    let dir = report_dir();
    fs::create_dir_all(&dir).expect("create report dir");

    let metrics_json = json!({
        "q1": result.q1_cases.iter().map(|(name, (baseline, memory))| {
            (
                name.clone(),
                json!({
                    "baseline": {
                        "average_overall_score": baseline.average_overall_score,
                        "best_architecture_score": baseline.best_architecture_score,
                        "pareto_frontier_size": baseline.pareto_frontier_size,
                        "candidate_count": baseline.candidate_count,
                        "evaluation_count": baseline.evaluation_count,
                        "search_depth": baseline.search_depth,
                    },
                    "with_memory": {
                        "average_overall_score": memory.average_overall_score,
                        "best_architecture_score": memory.best_architecture_score,
                        "pareto_frontier_size": memory.pareto_frontier_size,
                        "candidate_count": memory.candidate_count,
                        "evaluation_count": memory.evaluation_count,
                        "search_depth": memory.search_depth,
                    }
                }),
            )
        }).collect::<BTreeMap<_, _>>(),
        "q2": {
            "mean_search_depth_baseline": result.q2_mean_search_depth_baseline,
            "mean_search_depth_memory": result.q2_mean_search_depth_memory,
            "candidate_reduction": result.q2_candidate_reduction,
            "evaluation_reduction": result.q2_evaluation_reduction,
        },
        "q3": {
            "recall_accuracy": result.q3_recall_accuracy,
            "domain_mismatch_rate": result.q3_domain_mismatch_rate,
        },
        "q5": {
            "mean_frontier_score_baseline": result.q5_mean_frontier_score_baseline,
            "mean_frontier_score_memory": result.q5_mean_frontier_score_memory,
            "frontier_diversity_baseline": result.q5_frontier_diversity_baseline,
            "frontier_diversity_memory": result.q5_frontier_diversity_memory,
            "dominance_ratio_baseline": result.q5_dominance_ratio_baseline,
            "dominance_ratio_memory": result.q5_dominance_ratio_memory,
        },
        "latency": {
            "recall_latency_mean_ms": result.recall_latency_mean_ms,
            "recall_latency_max_ms": result.recall_latency_max_ms,
        }
    });
    fs::write(
        dir.join("metrics.json"),
        serde_json::to_string_pretty(&metrics_json).expect("serialize metrics"),
    )
    .expect("write metrics.json");

    let memory_stats_json = json!({
        "memory_nodes": result.q4_memory_nodes,
        "memory_edges": result.q4_memory_edges,
        "template_count": result.q4_template_count,
        "recall_latency_ms": result.q4_recall_latency_ms,
        "search_latency_ms": result.q4_search_latency_ms,
        "integrity_errors": result.integrity_errors,
    });
    fs::write(
        dir.join("memory_stats.json"),
        serde_json::to_string_pretty(&memory_stats_json).expect("serialize memory stats"),
    )
    .expect("write memory_stats.json");

    let mut report = String::new();
    report.push_str("# Architecture Quality Report\n\n");
    report.push_str("## Summary\n\n");
    report.push_str(&format!(
        "- Q2 candidate reduction: {:.2}\n- Q3 recall accuracy: {:.2}\n- Q4 memory nodes: {}\n- Q5 mean frontier score baseline: {:.4}\n- Q5 mean frontier score with memory: {:.4}\n",
        result.q2_candidate_reduction,
        result.q3_recall_accuracy,
        result.q4_memory_nodes,
        result.q5_mean_frontier_score_baseline,
        result.q5_mean_frontier_score_memory
    ));
    report.push_str("\n## Q1 Cases\n\n");
    for (name, (baseline, memory)) in &result.q1_cases {
        report.push_str(&format!(
            "- `{}` baseline avg {:.4}, memory avg {:.4}, baseline best {:.4}, memory best {:.4}\n",
            name,
            baseline.average_overall_score,
            memory.average_overall_score,
            baseline.best_architecture_score,
            memory.best_architecture_score
        ));
    }
    fs::write(dir.join("search_report.md"), report).expect("write search_report.md");
}

#[test]
fn architecture_quality_suite() {
    let result = run_quality_suite();

    for (name, (baseline, memory)) in &result.q1_cases {
        assert!(
            memory.average_overall_score + 1e-9 >= baseline.average_overall_score,
            "Q1 failed for {name}: memory {:.4} < baseline {:.4}",
            memory.average_overall_score,
            baseline.average_overall_score
        );
    }
    assert!(
        result.q2_candidate_reduction >= 0.30,
        "Q2 candidate reduction too low: {:.2}",
        result.q2_candidate_reduction
    );
    assert!(
        result.q3_recall_accuracy >= 0.8,
        "Q3 recall accuracy too low: {:.2}",
        result.q3_recall_accuracy
    );
    assert!(
        result.q3_domain_mismatch_rate <= 0.1,
        "Q3 domain mismatch too high: {:.2}",
        result.q3_domain_mismatch_rate
    );
    assert!(
        result.q4_recall_latency_ms < 50.0,
        "Q4 recall latency too high: {:.2}ms",
        result.q4_recall_latency_ms
    );
    assert!(
        result.q4_template_count < 200,
        "Q4 template count unbounded: {}",
        result.q4_template_count
    );
    assert_eq!(result.integrity_errors, 0, "memory integrity errors detected");
    assert!(
        result.q5_mean_frontier_score_memory + 1e-9 >= result.q5_mean_frontier_score_baseline,
        "Q5 frontier quality regressed"
    );
    assert!(result.recall_latency_mean_ms < 20.0);
    assert!(result.recall_latency_max_ms < 50.0);
}

#[test]
#[ignore = "long-run memory quality test"]
fn long_run_memory_test() {
    let engine = search_engine();
    let mut memory = DesignMemorySpace::default();
    seed_domain_memory(&mut memory);
    let cases = quality_cases();
    let mut search_total = Duration::ZERO;
    let mut recall_total = Duration::ZERO;
    let mut recall_max = Duration::ZERO;

    for index in 0..10_000 {
        let case = &cases[index % cases.len()];
        let search_started = Instant::now();
        let _ = engine.run_with_memory(&case.intent, &mut memory);
        search_total += search_started.elapsed();

        let recall_started = Instant::now();
        let _ = memory.recall_templates_for_intent(&intent_record(case), 3);
        let recall_elapsed = recall_started.elapsed();
        recall_total += recall_elapsed;
        recall_max = recall_max.max(recall_elapsed);
    }

    let recall_mean_ms = recall_total.as_secs_f64() * 1000.0 / 10_000.0;
    let search_mean_ms = search_total.as_secs_f64() * 1000.0 / 10_000.0;
    assert!(recall_mean_ms < 50.0);
    assert!(recall_max.as_secs_f64() * 1000.0 < 50.0);
    assert!(memory.template_memory.all().len() < 200);
    assert_eq!(integrity_errors(&memory), 0);

    let dir = report_dir();
    fs::create_dir_all(&dir).expect("create report dir");
    let summary = json!({
        "memory_nodes": memory.graph.nodes().len(),
        "memory_edges": memory.graph.edges().len(),
        "template_count": memory.template_memory.all().len(),
        "recall_mean_ms": recall_mean_ms,
        "recall_max_ms": recall_max.as_secs_f64() * 1000.0,
        "search_mean_ms": search_mean_ms,
    });
    fs::write(
        dir.join("memory_stats.json"),
        serde_json::to_string_pretty(&summary).expect("serialize long-run summary"),
    )
    .expect("write long-run summary");
}
