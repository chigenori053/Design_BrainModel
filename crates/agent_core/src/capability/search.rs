use std::collections::{BTreeMap, VecDeque};

use core_types::ObjectiveVector;
use field_engine::{FieldEngine, FieldVector};
use hybrid_vm::{HybridVM, StructuralEvaluator};
use memory_space::DesignState;

use crate::domain::DomainError;
use crate::domain::{AgentEvent, Hypothesis, Score};
use crate::capability::ScoringCapability;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub snippet: String,
}

pub trait SearchCapability: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<SearchHit>, DomainError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchCoreResult {
    pub best: Hypothesis,
    pub trace: Vec<crate::TraceRow>,
    pub events: Vec<AgentEvent>,
}

pub fn rank_hits_with_scorer<S: ScoringCapability>(
    hits: &[SearchHit],
    scorer: &S,
) -> Vec<(SearchHit, f64)> {
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

pub fn execute_soft_search_core(
    config: crate::TraceRunConfig,
    params: crate::SoftTraceParams,
) -> SearchCoreResult {
    let shm = HybridVM::default_shm();
    let _chm = crate::runtime::trace_helpers::make_dense_trace_chm(&shm, config.seed);
    let field = FieldEngine::new(256);
    let mut hybrid_vm = match HybridVM::with_default_memory(StructuralEvaluator::default()) {
        Ok(vm) => vm,
        Err(err) => {
            return SearchCoreResult {
                best: Hypothesis {
                    id: "soft-trace-init-error".to_string(),
                    content: format!("hybrid vm init failed: {err}"),
                },
                trace: Vec::new(),
                events: vec![AgentEvent::EmitTelemetry(crate::domain::TelemetryEvent {
                    name: "trace.hybrid_vm.init_error".to_string(),
                    value: err.to_string(),
                })],
            };
        }
    };

    let mut frontier = vec![crate::runtime::trace_helpers::trace_initial_state(config.seed)];
    let mut rows = Vec::with_capacity(config.depth);
    let mut lambda = 0.5f64;
    let mut field_cache: BTreeMap<(u128, u128, usize, usize), FieldVector> = BTreeMap::new();
    let mut field_cache_order: VecDeque<(u128, u128, usize, usize)> = VecDeque::new();
    let mut estimator = crate::GlobalRobustEstimator::default();
    let warmup_depths = 10usize;
    let mut events = Vec::new();

    let initial_alpha = if config.adaptive_alpha {
        if config.norm_alpha > 1e-6 {
            config.norm_alpha
        } else {
            0.01
        }
    } else {
        config.norm_alpha
    };
    let mut adaptive_state = crate::AdaptiveAlphaState::new(initial_alpha);

    for depth in 1..=config.depth {
        let calls_start = crate::DISTANCE_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let nn_calls_start = crate::NN_DISTANCE_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let norm_alpha_val = if config.adaptive_alpha {
            adaptive_state.alpha
        } else {
            config.norm_alpha
        };
        let mu = 0.0f64;
        let batch = crate::runtime::trace_helpers::build_soft_candidates_for_frontier(
            &mut hybrid_vm,
            &frontier,
            config.beam.max(1),
            depth,
            crate::runtime::trace_helpers::SoftSelectionParams {
                alpha: params.alpha,
                temperature: params.temperature,
                entropy_beta: params.entropy_beta,
            },
            crate::runtime::trace_helpers::SoftCandidateContext {
                field: &field,
                shm: &shm,
                field_profile: params.field_profile,
            },
            &mut field_cache,
            &mut field_cache_order,
        );
        let candidates = batch.candidates;
        let expanded_categories_count = batch.depth_category_counts.len();
        let per_category_selected = crate::runtime::trace_helpers::format_category_counts(&batch.depth_category_counts);
        let entropy_per_depth = crate::runtime::trace_helpers::shannon_entropy_from_counts(&batch.depth_category_counts) as f32;
        let depth_selected_rules_count = batch.depth_selected_rules_count;
        let field_extract_us = batch.field_extract_us;
        let field_score_us = batch.field_score_us;
        let field_aggregate_us = batch.field_aggregate_us;
        let field_total_us = batch.field_total_us;

        if let Some(path) = &config.raw_output_path {
            let objectives = candidates.iter().map(|(_, obj)| obj.clone()).collect::<Vec<_>>();
            events.push(AgentEvent::WriteRawObjectives {
                path: path.clone(),
                depth,
                objectives,
            });
        }

        if depth <= warmup_depths {
            estimator
                .samples
                .extend(candidates.iter().map(|(_, o)| crate::ObjectiveRaw(crate::runtime::trace_helpers::obj_to_arr(o))));
            if depth == warmup_depths {
                estimator.frozen = crate::runtime::trace_helpers::robust_stats_from_samples(&estimator.samples, norm_alpha_val);
            }
        }
        let stats = estimator
            .frozen
            .clone()
            .or_else(|| crate::runtime::trace_helpers::robust_stats_from_samples(&estimator.samples, norm_alpha_val))
            .unwrap_or(crate::GlobalRobustStats {
                alpha_used: norm_alpha_val,
                median: [0.0; 4],
                mad: [1.0; 4],
                mean: [0.0; 4],
                std: [1.0; 4],
                active_dims: [true; 4],
                weak_dims: [false; 4],
                weights: [1.0; 4],
                mad_zero_count: 0,
            });

        let lambda_old = lambda;
        lambda = crate::runtime::trace_helpers::update_lambda_entropy(
            lambda,
            entropy_per_depth as f64,
            params.lambda_target_entropy,
            params.lambda_k,
            params.lambda_ema,
            params.lambda_min,
            1.0,
        );

        if candidates.is_empty() {
            let _ = hybrid_vm.take_memory_telemetry();
            rows.push(crate::TraceRow {
                depth,
                lambda: lambda as f32,
                delta_lambda: (lambda - lambda_old) as f32,
                tau_prime: 0.0,
                conf_chm: 0.0,
                density: 0.0,
                k: 0,
                h_profile: 1.0,
                pareto_size: 0,
                diversity: 0.0,
                resonance_avg: 0.0,
                pressure: 0.0,
                epsilon_effect: 0.0,
                target_local_weight: 0.5,
                target_global_weight: 0.5,
                local_global_distance: 0.0,
                field_min_distance: 0.0,
                field_rejected_count: if depth == 1 { 1 } else { 0 },
                mu: mu as f32,
                dhm_k: 0,
                dhm_norm: 0.0,
                dhm_resonance_mean: 0.0,
                dhm_score_ratio: 1.0,
                dhm_build_us: 0.0,
                expanded_categories_count,
                selected_rules_count: depth_selected_rules_count,
                per_category_selected,
                entropy_per_depth,
                unique_category_count_per_depth: expanded_categories_count,
                pareto_front_size_per_depth: 0,
                pareto_mean_nn_dist: 0.0,
                pareto_spacing: 0.0,
                pareto_hv_2d: 0.0,
                field_extract_us: field_extract_us as f32,
                field_score_us: field_score_us as f32,
                field_aggregate_us: field_aggregate_us as f32,
                field_total_us: field_total_us as f32,
                norm_median_0: stats.median[0] as f32,
                norm_median_1: stats.median[1] as f32,
                norm_median_2: stats.median[2] as f32,
                norm_median_3: stats.median[3] as f32,
                norm_mad_0: stats.mad[0] as f32,
                norm_mad_1: stats.mad[1] as f32,
                norm_mad_2: stats.mad[2] as f32,
                norm_mad_3: stats.mad[3] as f32,
                median_nn_dist_all_depth: 0.0,
                collapse_flag: false,
                normalization_mode: "global_robust".to_string(),
                unique_norm_vec_count: 0,
                norm_dim_mad_zero_count: 0,
                mean_nn_dist_raw: 0.0,
                mean_nn_dist_norm: 0.0,
                pareto_spacing_raw: 0.0,
                pareto_spacing_norm: 0.0,
                distance_calls: 0,
                nn_distance_calls: 0,
                weak_dim_count: 0,
                effective_dim_count: 0,
                alpha_t: 0.0,
                weak_contrib_ratio: 0.0,
                collapse_proxy: 0.0,
                avg_tau_mem: 0.0,
                avg_delta_norm: 0.0,
                memory_hit_rate: 0.0,
                redundancy_flags: String::new(),
                saturation_flags: String::new(),
                discrete_saturation_count: 0,
                effective_dim: 0,
                effective_dim_ratio: 0.0,
                collapse_reasons: String::new(),
            });
            continue;
        }

        let (normalized, _) = crate::normalize_by_depth(candidates, norm_alpha_val);
        let norm_data: Vec<[f64; 4]> = normalized.iter().map(|(_, obj)| crate::runtime::trace_helpers::obj_to_arr(obj)).collect();
        let front = crate::capability::selection::soft_front_rank(normalized, crate::SOFT_PARETO_TEMPERATURE);

        if front.is_empty() {
            let _ = hybrid_vm.take_memory_telemetry();
            frontier = vec![crate::runtime::trace_helpers::trace_initial_state(config.seed)];
            continue;
        }

        let front_norm = front
            .iter()
            .map(|(_, o)| {
                crate::engine::pareto::normalize_objective(
                    &crate::ObjectiveRaw(crate::runtime::trace_helpers::obj_to_arr(o)),
                    &stats,
                )
            })
            .collect::<Vec<_>>();
        let depth_boundary_diversity = crate::runtime::trace_helpers::variance(
            &front
                .iter()
                .map(|(_, o)| crate::scalar_score(o))
                .collect::<Vec<_>>(),
        );
        let resonance_avg = front.iter().map(|(_, o)| o.f_field).sum::<f64>() / front.len() as f64;
        let pareto_mean_nn = crate::engine::pareto::mean_nn_dist_norm(&front_norm, &stats.weights);
        let pareto_spacing = crate::engine::pareto::spacing_norm(&front_norm, &stats.weights);
        let pareto_hv_2d = crate::engine::pareto::pareto_hv_2d_norm(&front_norm);
        let unique_norm_vec_count =
            crate::engine::pareto::count_unique_norm(&front_norm, &stats.weights);
        let s_count = stats
            .active_dims
            .iter()
            .zip(stats.weak_dims.iter())
            .filter(|&(&a, &w)| a && !w)
            .count();
        let w_count = stats.weak_dims.iter().filter(|&&w| w).count();
        let effective_dim_count = s_count + w_count;
        let weak_dim_count = w_count;
        let weak_contrib_ratio = if w_count > 0 {
            norm_alpha_val * (w_count as f64)
                / ((s_count as f64) + norm_alpha_val * (w_count as f64))
        } else {
            0.0
        };
        let collapse_proxy = if depth > warmup_depths && front.len() > 1 && pareto_mean_nn < 0.01 {
            1.0
        } else {
            0.0
        };

        let stability_metrics = crate::ObjectiveStabilityAnalyzer::analyze(
            &norm_data,
            &stats.mad,
            unique_norm_vec_count,
            pareto_mean_nn,
        );

        if config.adaptive_alpha && depth > warmup_depths {
            adaptive_state = crate::calculate_adaptive_alpha(
                &adaptive_state,
                &stats,
                pareto_mean_nn,
                front.len(),
                0.01,
                stability_metrics.effective_dim,
            );
        }
        let norm_dim_mad_zero_count = stats.mad.iter().filter(|&&m| m.abs() < 1e-9).count();

        let front_refs: Vec<&ObjectiveVector> = front.iter().map(|(_, o)| o).collect();
        let pareto_mean_nn_raw = crate::engine::pareto::pareto_mean_nn_distance(&front_refs);
        let pareto_spacing_raw = crate::engine::pareto::pareto_spacing_metric(&front_refs);

        let calls_end = crate::DISTANCE_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let nn_calls_end = crate::NN_DISTANCE_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let distance_calls = calls_end.saturating_sub(calls_start);
        let nn_distance_calls = nn_calls_end.saturating_sub(nn_calls_start);
        let mem_telemetry = hybrid_vm.take_memory_telemetry();

        rows.push(crate::TraceRow {
            depth,
            lambda: lambda as f32,
            delta_lambda: (lambda - lambda_old) as f32,
            tau_prime: 0.0,
            conf_chm: 0.0,
            density: 0.0,
            k: 0,
            h_profile: 1.0,
            pareto_size: front.len(),
            diversity: depth_boundary_diversity as f32,
            resonance_avg: resonance_avg as f32,
            pressure: 0.0,
            epsilon_effect: 0.0,
            target_local_weight: 0.5,
            target_global_weight: 0.5,
            local_global_distance: 0.0,
            field_min_distance: 0.0,
            field_rejected_count: if depth == 1 { 1 } else { 0 },
            mu: mu as f32,
            dhm_k: 0,
            dhm_norm: 0.0,
            dhm_resonance_mean: 0.0,
            dhm_score_ratio: 1.0,
            dhm_build_us: 0.0,
            expanded_categories_count,
            selected_rules_count: depth_selected_rules_count,
            per_category_selected,
            entropy_per_depth,
            unique_category_count_per_depth: expanded_categories_count,
            pareto_front_size_per_depth: front.len(),
            pareto_mean_nn_dist: pareto_mean_nn as f32,
            pareto_spacing: pareto_spacing as f32,
            pareto_hv_2d: pareto_hv_2d as f32,
            field_extract_us: field_extract_us as f32,
            field_score_us: field_score_us as f32,
            alpha_t: norm_alpha_val as f32,
            weak_contrib_ratio: weak_contrib_ratio as f32,
            collapse_proxy: collapse_proxy as f32,
            avg_tau_mem: mem_telemetry.avg_tau_mem as f32,
            avg_delta_norm: mem_telemetry.avg_delta_norm as f32,
            memory_hit_rate: mem_telemetry.memory_hit_rate as f32,
            field_aggregate_us: field_aggregate_us as f32,
            field_total_us: field_total_us as f32,
            norm_median_0: stats.median[0] as f32,
            norm_median_1: stats.median[1] as f32,
            norm_median_2: stats.median[2] as f32,
            norm_median_3: stats.median[3] as f32,
            norm_mad_0: stats.mad[0] as f32,
            norm_mad_1: stats.mad[1] as f32,
            norm_mad_2: stats.mad[2] as f32,
            norm_mad_3: stats.mad[3] as f32,
            median_nn_dist_all_depth: 0.0,
            collapse_flag: false,
            normalization_mode: "global_robust".to_string(),
            unique_norm_vec_count,
            norm_dim_mad_zero_count,
            mean_nn_dist_raw: pareto_mean_nn_raw as f32,
            mean_nn_dist_norm: pareto_mean_nn as f32,
            pareto_spacing_raw: pareto_spacing_raw as f32,
            pareto_spacing_norm: pareto_spacing as f32,
            distance_calls,
            nn_distance_calls,
            weak_dim_count,
            effective_dim_count,
            redundancy_flags: stability_metrics.redundancy_flags.join("|"),
            saturation_flags: stability_metrics.saturation_flags.join("|"),
            discrete_saturation_count: stability_metrics.discrete_saturation_count,
            effective_dim: stability_metrics.effective_dim,
            effective_dim_ratio: stability_metrics.effective_dim_ratio as f32,
            collapse_reasons: stability_metrics.collapse_reasons.join("|"),
        });

        frontier = crate::engine::pareto::select_beam_maxmin_norm(
            front,
            front_norm,
            config.beam.max(1),
            &stats.weights,
        )
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<DesignState>>();
        if frontier.is_empty() {
            frontier = vec![crate::runtime::trace_helpers::trace_initial_state(config.seed)];
        }
    }

    let all_nn = rows
        .iter()
        .map(|r| r.pareto_mean_nn_dist as f64)
        .filter(|v| *v > 0.0)
        .collect::<Vec<_>>();
    let d_med = crate::runtime::trace_helpers::median(all_nn);
    for row in &mut rows {
        row.median_nn_dist_all_depth = d_med as f32;
        row.collapse_flag =
            (row.pareto_mean_nn_dist as f64) < 0.01 * d_med && row.pareto_front_size_per_depth >= 2;
    }

    SearchCoreResult {
        best: Hypothesis {
            id: "soft-trace".to_string(),
            content: format!("rows={}", rows.len()),
        },
        trace: rows,
        events,
    }
}

pub fn execute_trace_core(config: crate::TraceRunConfig) -> SearchCoreResult {
    execute_baseline_off_core(config)
}

pub fn execute_baseline_off_core(config: crate::TraceRunConfig) -> SearchCoreResult {
    let trace = execute_soft_search_core(config, crate::SoftTraceParams::default()).trace;
    SearchCoreResult {
        best: Hypothesis {
            id: "baseline-off".to_string(),
            content: format!("rows={}", trace.len()),
        },
        trace,
        events: vec![
            AgentEvent::PersistMemory {
                key: "trace/baseline_off".to_string(),
                value: b"completed".to_vec(),
            },
            AgentEvent::EmitTelemetry(crate::domain::TelemetryEvent {
                name: "trace.baseline_off.completed".to_string(),
                value: "1".to_string(),
            }),
        ],
    }
}

pub fn execute_balanced_core(config: crate::TraceRunConfig, m: usize) -> SearchCoreResult {
    let mut params = crate::SoftTraceParams::default();
    params.alpha = (m as f64 / 10.0).clamp(0.1, 1.0);
    let trace = execute_soft_search_core(config, params).trace;
    SearchCoreResult {
        best: Hypothesis {
            id: "balanced".to_string(),
            content: format!("rows={}", trace.len()),
        },
        trace,
        events: vec![
            AgentEvent::PersistMemory {
                key: "trace/balanced".to_string(),
                value: b"completed".to_vec(),
            },
            AgentEvent::EmitTelemetry(crate::domain::TelemetryEvent {
                name: "trace.balanced.completed".to_string(),
                value: "1".to_string(),
            }),
        ],
    }
}
