use design_reasoning::{
    DesignFactor, FactorType, ScsInputs, compute_dependency_consistency_metrics, compute_scs_v1_1,
    sanitize_factors,
};
use std::collections::BTreeMap;

pub const ENGINE_VERSION: &str = design_reasoning::Phase1Engine::ENGINE_VERSION;

pub fn run_phase1_matrix(
    config: crate::Phase1Config,
) -> (Vec<crate::Phase1RawRow>, Vec<crate::Phase1SummaryRow>) {
    run_phase1_matrix_impl(config)
}

fn run_phase1_matrix_impl(
    config: crate::Phase1Config,
) -> (Vec<crate::Phase1RawRow>, Vec<crate::Phase1SummaryRow>) {
    let variants = [
        crate::Phase1Variant::Base,
        crate::Phase1Variant::Delta,
        crate::Phase1Variant::Ortho { epsilon: 0.02 },
    ];
    let mut raw = Vec::new();
    let mut summary = Vec::new();
    for variant in variants {
        let (r, s) = run_phase1_variant(config, variant);
        raw.extend(r);
        summary.extend(s);
    }
    (raw, summary)
}

fn run_phase1_variant(
    config: crate::Phase1Config,
    variant: crate::Phase1Variant,
) -> (Vec<crate::Phase1RawRow>, Vec<crate::Phase1SummaryRow>) {
    const HV_STOP_WINDOW: usize = 10;
    const HV_STOP_EPS: f64 = 1e-6;
    let shm = hybrid_vm::HybridVM::default_shm();
    let chm = crate::runtime::trace_helpers::make_dense_trace_chm(&shm, config.seed);
    let field = field_engine::FieldEngine::new(256);
    let mut hybrid_vm =
        match hybrid_vm::HybridVM::with_default_memory(hybrid_vm::StructuralEvaluator::default()) {
            Ok(vm) => vm,
            Err(_) => return (Vec::new(), Vec::new()),
        };
    let mut frontier = vec![crate::runtime::trace_helpers::trace_initial_state(
        config.seed,
    )];
    let mut lambda = 0.5f64;
    let mut field_cache: std::collections::BTreeMap<
        (u128, u128, usize, usize),
        field_engine::FieldVector,
    > = std::collections::BTreeMap::new();
    let mut field_cache_order: std::collections::VecDeque<(u128, u128, usize, usize)> =
        std::collections::VecDeque::new();
    let mut raw_rows = Vec::new();
    let mut summary_rows = Vec::new();
    let mut delta_hv_window = std::collections::VecDeque::<f64>::new();
    let mut previous_frontier_hv = 0.0f64;
    let mut search_controller = None::<crate::runtime::structured_search::SearchController>;
    let mut simulation_cache = crate::runtime::world_model::SimulationCache::new();
    let mut learning_engine = crate::runtime::world_model::LearningEngine::new(
        config.world_model_mode,
        config.world_model_learning_rate,
        config.world_model_learning_decay,
        config.world_model_learning_confidence_gate,
    );

    for depth in 1..=config.max_steps.max(1) {
        let target_field = crate::build_target_field(&field, &shm, &frontier[0], lambda);
        let mut depth_category_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut candidates: Vec<(
            memory_space::DesignState,
            core_types::ObjectiveVector,
            memory_space::Uuid,
        )> = Vec::new();

        for (state_idx, state) in frontier.iter().enumerate() {
            let (selected_rules, _, _) = crate::runtime::trace_helpers::select_rules_category_soft(
                hybrid_vm::HybridVM::applicable_rules(&shm, state),
                (config.beam_width.max(1) * 5).max(1),
                config.alpha,
                config.temperature,
                config.entropy_beta,
            );
            let current_obj =
                evaluate_state_for_phase1(state, &mut hybrid_vm, &chm, &field, &target_field);
            for rule in selected_rules {
                *depth_category_counts
                    .entry(
                        crate::runtime::trace_helpers::rule_category_name(&rule.category)
                            .to_string(),
                    )
                    .or_insert(0) += 1;
                let new_state = crate::apply_atomic(rule, state);
                let key = (new_state.id.as_u128(), rule.id.as_u128(), depth, state_idx);
                let _ = crate::runtime::trace_helpers::bounded_cache_get_or_insert(
                    &mut field_cache,
                    &mut field_cache_order,
                    key,
                    || field.aggregate_state(&new_state),
                )
                .0;
                let obj = hybrid_vm.evaluate(&new_state);
                let obj = match variant {
                    crate::Phase1Variant::Base => obj.clamped(),
                    crate::Phase1Variant::Delta => objective_delta(&obj, &current_obj),
                    crate::Phase1Variant::Ortho { epsilon } => {
                        objective_with_ortho(&new_state, obj, epsilon)
                    }
                };
                candidates.push((new_state, obj, rule.id));
            }
        }

        if candidates.is_empty() {
            break;
        }

        let mut front_map: std::collections::BTreeMap<
            memory_space::StateId,
            (
                memory_space::DesignState,
                core_types::ObjectiveVector,
                memory_space::Uuid,
            ),
        > = std::collections::BTreeMap::new();
        for (state, obj, rid) in candidates {
            front_map.entry(state.id).or_insert((state, obj, rid));
        }
        let front_entries: Vec<(
            memory_space::DesignState,
            core_types::ObjectiveVector,
            memory_space::Uuid,
        )> = front_map.into_values().collect();
        let world_mode_entries = front_entries
            .into_iter()
            .map(|(state, obj, rid)| {
                if config.world_model_enabled {
                    if let Some(simulated) = crate::runtime::world_model::simulate_best_action(
                        &state,
                        obj.clone(),
                        &mut hybrid_vm,
                        config.world_model_alpha,
                        config.world_model_beta,
                        config.world_model_beta_profile,
                        config.world_model_actions_per_state,
                        config.world_model_max_depth,
                        config.intent_profile,
                        config.world_model_mode,
                        config.world_model_variance_penalty,
                        config.world_model_semantic_variance_penalty,
                        config.world_model_semantic_variance_max_penalty,
                        config.world_model_confidence_floor,
                        &mut learning_engine,
                        search_controller
                            .as_ref()
                            .map(|controller| &controller.metrics),
                        &mut simulation_cache,
                    ) {
                        return (
                            simulated.state,
                            simulated.objective,
                            rid,
                            Some(simulated.delta),
                        );
                    }
                }
                (state, obj, rid, None)
            })
            .collect::<Vec<_>>();
        let normalized_depth = normalize_phase1_vectors(
            &world_mode_entries
                .iter()
                .map(|(_, o, _, _)| o.clone())
                .collect::<Vec<_>>(),
        );
        let structured_candidates = world_mode_entries
            .iter()
            .zip(normalized_depth.iter())
            .map(|((state, obj, rid, simulation), norm)| {
                crate::runtime::structured_search::build_search_candidate(
                    state.clone(),
                    obj.clone(),
                    *rid,
                    *norm,
                    simulation.clone(),
                )
            })
            .collect::<Vec<_>>();
        let structured_outcome = crate::runtime::structured_search::select_controlled_frontier(
            structured_candidates.clone(),
            search_controller.as_ref(),
            config.beam_width.max(1).min(world_mode_entries.len()),
            depth,
            config.max_steps.max(1),
        );

        let front = structured_outcome
            .controller
            .frontier
            .iter()
            .map(|candidate| {
                (
                    candidate.state.clone(),
                    candidate.objective.clone(),
                    candidate.rule_id,
                )
            })
            .collect::<Vec<_>>();
        let normalized_front = structured_outcome
            .controller
            .frontier
            .iter()
            .map(|candidate| candidate.normalized_objective)
            .collect::<Vec<_>>();
        let cluster_map = cluster_membership_map(
            &structured_outcome.controller.clusters,
            &structured_candidates,
        );
        let diversity_scores = frontier_diversity_scores(&structured_outcome.controller.frontier);
        let corr = corr_matrix4(&normalized_front);
        let mean_nn = mean_nn_dist4(&normalized_front);
        let spacing = spacing4(&normalized_front);
        let collapse_flag = mean_nn < 1e-4 && front.len() >= 2;
        summary_rows.push(crate::Phase1SummaryRow {
            variant: variant.name().to_string(),
            depth,
            corr_matrix_flat: flatten_corr4(&corr),
            mean_nn_dist: mean_nn,
            spacing,
            pareto_front_size: front.len(),
            frontier_hv: structured_outcome.frontier_hv,
            hv_delta: structured_outcome.hv_delta,
            beta_used: structured_outcome.beta_used,
            semantic_variance_mean: structured_outcome
                .controller
                .frontier
                .iter()
                .filter_map(|candidate| {
                    candidate
                        .simulation
                        .as_ref()
                        .map(|delta| delta.semantic_variance)
                })
                .sum::<f64>()
                / structured_outcome
                    .controller
                    .frontier
                    .iter()
                    .filter(|candidate| candidate.simulation.is_some())
                    .count()
                    .max(1) as f64,
            world_model_enabled: config.world_model_enabled,
            cluster_count: structured_outcome.controller.clusters.len(),
            cluster_coverage: structured_outcome.cluster_coverage,
            score_variance: structured_outcome.score_variance,
            diversity_mean: structured_outcome.diversity_mean,
            frontier_change_ratio: structured_outcome.frontier_change_ratio,
            stagnation_steps: structured_outcome.controller.metrics.stagnation_steps,
            stop_triggered: structured_outcome.stop_triggered,
            cluster_collapse_flag: structured_outcome.cluster_collapsed,
            collapse_flag,
        });

        let beam_take = config.beam_width.max(1).min(front.len());
        let mut id_to_norm: std::collections::BTreeMap<u128, [f64; 4]> =
            std::collections::BTreeMap::new();
        let simulation_by_state = structured_outcome
            .controller
            .frontier
            .iter()
            .filter_map(|candidate| {
                candidate
                    .simulation
                    .as_ref()
                    .map(|simulation| (candidate.state.id.as_u128(), simulation.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        for ((state, _, _), norm) in front.iter().zip(normalized_front.iter()) {
            id_to_norm.insert(state.id.as_u128(), *norm);
        }
        for (beam_index, (state, obj, rid)) in front.iter().take(beam_take).enumerate() {
            let norm = id_to_norm
                .get(&state.id.as_u128())
                .copied()
                .unwrap_or([0.0; 4]);
            let cluster_id = cluster_map
                .get(&state.id.as_u128())
                .copied()
                .unwrap_or_default();
            let diversity_score = diversity_scores
                .get(&state.id.as_u128())
                .copied()
                .unwrap_or(0.0);
            let simulation = simulation_by_state.get(&state.id.as_u128()).cloned();
            let factors = build_design_factors(state);
            let (factors, sanity) = sanitize_factors(&factors);
            let dep_metrics = compute_dependency_consistency_metrics(&factors);
            let completeness = obj.f_struct.clamp(0.0, 1.0);
            let ambiguity_mean = (1.0 - obj.f_field).clamp(0.0, 1.0);
            let inconsistency = obj.f_risk.clamp(0.0, 1.0);
            let cls = ambiguity_mean;
            let scs_v1 = crate::scalar_score(obj).clamp(0.0, 1.0);
            let scs_v1_1 = compute_scs_v1_1(ScsInputs {
                completeness,
                ambiguity_mean,
                dependency_consistency: dep_metrics.dependency_consistency,
                inconsistency,
            });
            let phase2_triggered = scs_v1_1 >= 0.72 && cls <= 0.50 && inconsistency <= 0.40;
            let phase2_false_trigger_proxy =
                phase2_triggered && dep_metrics.dependency_consistency < 0.50;
            raw_rows.push(crate::Phase1RawRow {
                variant: variant.name().to_string(),
                depth,
                beam_index,
                cluster_id,
                rule_id: format!("{:032x}", rid.as_u128()),
                objective_vector_raw: fmt_vec4(&crate::runtime::trace_helpers::obj_to_arr(obj)),
                objective_vector_norm: fmt_vec4(&norm),
                diversity_score,
                action_label: simulation
                    .as_ref()
                    .map(|delta| delta.action_label.clone())
                    .unwrap_or_else(|| "static".to_string()),
                repair_potential: simulation.as_ref().map(|delta| delta.rp).unwrap_or(0.0),
                intent_score: simulation
                    .as_ref()
                    .map(|delta| delta.intent_score)
                    .unwrap_or(0.0),
                confidence: simulation
                    .as_ref()
                    .map(|delta| delta.confidence)
                    .unwrap_or(1.0),
                variance: simulation
                    .as_ref()
                    .map(|delta| delta.variance)
                    .unwrap_or(0.0),
                semantic_variance: simulation
                    .as_ref()
                    .map(|delta| delta.semantic_variance)
                    .unwrap_or(0.0),
                uncertainty: simulation
                    .as_ref()
                    .map(|delta| delta.uncertainty)
                    .unwrap_or(0.0),
                beta_reliance: simulation
                    .as_ref()
                    .map(|delta| delta.beta_reliance)
                    .unwrap_or(0.0),
                learning_bias: simulation
                    .as_ref()
                    .map(|delta| delta.learning_bias)
                    .unwrap_or(0.0),
                final_score: simulation
                    .as_ref()
                    .map(|delta| delta.final_score)
                    .unwrap_or(crate::scalar_score(obj).clamp(0.0, 1.0)),
                delta_violations: simulation
                    .as_ref()
                    .map(|delta| delta.delta_violations)
                    .unwrap_or(0.0),
                delta_coupling: simulation
                    .as_ref()
                    .map(|delta| delta.delta_coupling)
                    .unwrap_or(0.0),
                delta_propagation_score: simulation
                    .as_ref()
                    .map(|delta| delta.delta_propagation_score)
                    .unwrap_or(0.0),
                completeness,
                ambiguity_mean,
                inconsistency,
                cls,
                scs_v1,
                scs_v1_1,
                dependency_consistency: dep_metrics.dependency_consistency,
                connectivity: dep_metrics.connectivity,
                cyclicity: dep_metrics.cyclicity,
                orphan_rate: dep_metrics.orphan_rate,
                phase2_triggered,
                phase2_false_trigger_proxy,
                sanity_empty_id_fixes: sanity.empty_id_fixes,
                sanity_duplicate_id_fixes: sanity.duplicate_id_fixes,
                sanity_unknown_dependency_drops: sanity.unknown_dependency_drops,
            });
        }

        let entropy =
            crate::runtime::trace_helpers::shannon_entropy_from_counts(&depth_category_counts);
        lambda = crate::runtime::trace_helpers::update_lambda_entropy(
            lambda,
            entropy,
            config.lambda_target_entropy,
            config.lambda_k,
            config.lambda_ema,
            config.lambda_min,
            1.0,
        );
        if matches!(config.hv_policy, crate::HvPolicy::Guided) {
            let delta_hv_selected =
                (structured_outcome.frontier_hv - previous_frontier_hv).max(0.0);
            eprintln!(
                "hv_guided iteration={} current_HV={:.8} delta_HV_selected={:.8} frontier_size={} cluster_coverage={:.3}",
                depth,
                structured_outcome.frontier_hv,
                delta_hv_selected,
                front.len(),
                structured_outcome.cluster_coverage
            );
            delta_hv_window.push_back(delta_hv_selected);
            if delta_hv_window.len() > HV_STOP_WINDOW {
                delta_hv_window.pop_front();
            }
            frontier = front
                .into_iter()
                .take(beam_take)
                .map(|(s, _, _)| s)
                .collect();
        } else {
            frontier = front
                .into_iter()
                .take(beam_take)
                .map(|(s, _, _)| s)
                .collect();
        }
        previous_frontier_hv = structured_outcome.frontier_hv;
        search_controller = Some(structured_outcome.controller.clone());
        if frontier.is_empty() {
            frontier = vec![crate::runtime::trace_helpers::trace_initial_state(
                config.seed,
            )];
        }
        if structured_outcome.stop_triggered {
            break;
        }
        if matches!(config.hv_policy, crate::HvPolicy::Guided)
            && delta_hv_window.len() == HV_STOP_WINDOW
        {
            let mean_delta = delta_hv_window.iter().sum::<f64>() / HV_STOP_WINDOW as f64;
            if mean_delta < HV_STOP_EPS {
                break;
            }
        }
        let _ = normalized_depth;
    }

    (raw_rows, summary_rows)
}

fn fmt_vec4(v: &[f64; 4]) -> String {
    format!("{:.9}|{:.9}|{:.9}|{:.9}", v[0], v[1], v[2], v[3])
}

fn evaluate_state_for_phase1(
    state: &memory_space::DesignState,
    vm: &mut hybrid_vm::HybridVM,
    _chm: &hybrid_vm::Chm,
    _field: &field_engine::FieldEngine,
    _target: &field_engine::TargetField,
) -> core_types::ObjectiveVector {
    vm.evaluate(state).clamped()
}

fn objective_delta(
    next: &core_types::ObjectiveVector,
    current: &core_types::ObjectiveVector,
) -> core_types::ObjectiveVector {
    crate::runtime::trace_helpers::arr_to_obj([
        next.f_struct - current.f_struct,
        next.f_field - current.f_field,
        next.f_risk - current.f_risk,
        next.f_shape - current.f_shape,
    ])
}

fn objective_with_ortho(
    state: &memory_space::DesignState,
    obj: core_types::ObjectiveVector,
    eps: f64,
) -> core_types::ObjectiveVector {
    let nodes = state.graph.nodes().len() as f64;
    let edges = state.graph.edges().len() as f64;
    let hist = state.profile_snapshot.len() as f64;
    let g = [
        (nodes / 64.0).tanh(),
        (edges / 128.0).tanh(),
        ((nodes - edges).abs() / 64.0).tanh(),
        (hist / 256.0).tanh(),
    ];
    crate::runtime::trace_helpers::arr_to_obj([
        (obj.f_struct + eps * g[0]).clamp(0.0, 1.0),
        (obj.f_field + eps * g[1]).clamp(0.0, 1.0),
        (obj.f_risk + eps * g[2]).clamp(0.0, 1.0),
        (obj.f_shape + eps * g[3]).clamp(0.0, 1.0),
    ])
}

fn cluster_membership_map(
    clusters: &[crate::runtime::structured_search::Cluster],
    candidates: &[crate::runtime::structured_search::SearchCandidate],
) -> BTreeMap<u128, usize> {
    let mut out = BTreeMap::new();
    for cluster in clusters {
        for member in &cluster.members {
            if let Some(candidate) = candidates.get(*member) {
                out.insert(candidate.state.id.as_u128(), cluster.id);
            }
        }
    }
    out
}

fn frontier_diversity_scores(
    frontier: &[crate::runtime::structured_search::SearchCandidate],
) -> BTreeMap<u128, f64> {
    let mut out = BTreeMap::new();
    for (idx, candidate) in frontier.iter().enumerate() {
        let diversity = frontier
            .iter()
            .enumerate()
            .filter(|(other_idx, _)| *other_idx != idx)
            .map(|(_, other)| {
                let diffs = [
                    candidate.feature.coupling - other.feature.coupling,
                    candidate.feature.propagation_score - other.feature.propagation_score,
                    candidate.feature.impact - other.feature.impact,
                    candidate.feature.structural_variance - other.feature.structural_variance,
                    candidate.feature.cycle_flag - other.feature.cycle_flag,
                ];
                diffs.iter().map(|diff| diff * diff).sum::<f64>().sqrt() / 5.0f64.sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        out.insert(
            candidate.state.id.as_u128(),
            if diversity.is_finite() {
                diversity
            } else {
                1.0
            },
        );
    }
    out
}

fn normalize_phase1_vectors(objs: &[core_types::ObjectiveVector]) -> Vec<[f64; 4]> {
    if objs.is_empty() {
        return Vec::new();
    }
    let eps = 1e-6;
    let clip = 3.0;
    let margin = 1e-3;
    let arrs = objs
        .iter()
        .map(crate::runtime::trace_helpers::obj_to_arr)
        .collect::<Vec<_>>();
    let mut meds = [0.0; 4];
    let mut mads = [0.0; 4];
    for i in 0..4 {
        let col = arrs.iter().map(|v| v[i]).collect::<Vec<_>>();
        meds[i] = crate::runtime::trace_helpers::median(col.clone());
        let abs_dev = col.iter().map(|x| (x - meds[i]).abs()).collect::<Vec<_>>();
        mads[i] = crate::runtime::trace_helpers::median(abs_dev);
    }
    arrs.into_iter()
        .map(|v| {
            let mut out = [0.0; 4];
            for i in 0..4 {
                let z = ((v[i] - meds[i]) / (mads[i] + eps)).clamp(-clip, clip);
                let base = ((z + clip) / (2.0 * clip)).clamp(0.0, 1.0);
                out[i] = (margin + (1.0 - 2.0 * margin) * base).clamp(0.0, 1.0);
            }
            out
        })
        .collect()
}

fn build_design_factors(state: &memory_space::DesignState) -> Vec<DesignFactor> {
    let mut deps_by_id: std::collections::BTreeMap<u128, Vec<String>> =
        std::collections::BTreeMap::new();
    for node in state.graph.nodes().values() {
        deps_by_id.entry(node.id.as_u128()).or_default();
    }
    for (from, to) in state.graph.edges() {
        deps_by_id
            .entry(from.as_u128())
            .or_default()
            .push(format!("{:032x}", to.as_u128()));
    }
    for deps in deps_by_id.values_mut() {
        deps.sort();
        deps.dedup();
    }

    state
        .graph
        .nodes()
        .values()
        .map(|node| {
            let id = format!("{:032x}", node.id.as_u128());
            let factor_type = infer_factor_type(node);
            let depends_on = deps_by_id
                .get(&node.id.as_u128())
                .cloned()
                .unwrap_or_default();
            DesignFactor {
                id,
                factor_type,
                depends_on,
            }
        })
        .collect()
}

fn infer_factor_type(node: &memory_space::DesignNode) -> FactorType {
    let kind = node.kind.to_ascii_lowercase();
    if kind.contains("why") || kind.contains("goal") || kind.contains("objective") {
        return FactorType::Why;
    }
    if kind.contains("what") {
        return FactorType::What;
    }
    if kind.contains("how") || kind.contains("impl") || kind.contains("method") {
        return FactorType::How;
    }
    if kind.contains("constraint") || kind.contains("rule") {
        return FactorType::Constraint;
    }
    if kind.contains("risk") {
        return FactorType::Risk;
    }
    if let Some(memory_space::Value::Text(category)) = node.attributes.get("category") {
        let c = category.to_ascii_lowercase();
        if c.contains("structural") || c.contains("goal") {
            return FactorType::Why;
        }
        if c.contains("constraint") {
            return FactorType::Constraint;
        }
        if c.contains("reliability") || c.contains("risk") {
            return FactorType::Risk;
        }
    }
    FactorType::Unknown
}

#[allow(clippy::needless_range_loop)]
fn corr_matrix4(vs: &[[f64; 4]]) -> [[f64; 4]; 4] {
    let n = vs.len();
    if n < 2 {
        return [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
    }
    let mut mean = [0.0; 4];
    for v in vs {
        for i in 0..4 {
            mean[i] += v[i];
        }
    }
    for i in 0..4 {
        mean[i] /= n as f64;
    }
    let mut var = [0.0; 4];
    for v in vs {
        for i in 0..4 {
            var[i] += (v[i] - mean[i]).powi(2);
        }
    }
    for i in 0..4 {
        var[i] /= (n - 1) as f64;
    }
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            if i == j {
                out[i][j] = 1.0;
                continue;
            }
            if var[i] <= 1e-12 || var[j] <= 1e-12 {
                out[i][j] = 0.0;
                continue;
            }
            let mut cov = 0.0;
            for v in vs {
                cov += (v[i] - mean[i]) * (v[j] - mean[j]);
            }
            cov /= (n - 1) as f64;
            out[i][j] = (cov / (var[i].sqrt() * var[j].sqrt())).clamp(-1.0, 1.0);
        }
    }
    out
}

fn flatten_corr4(c: &[[f64; 4]; 4]) -> String {
    let mut vals = Vec::with_capacity(16);
    for row in c {
        for v in row {
            vals.push(format!("{:.6}", v));
        }
    }
    vals.join("|")
}

fn dist4(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let mut s = 0.0;
    for i in 0..4 {
        s += (a[i] - b[i]).powi(2);
    }
    s.sqrt()
}

fn mean_nn_dist4(vs: &[[f64; 4]]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(dist4(v, u));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / vs.len() as f64
}

fn spacing4(vs: &[[f64; 4]]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    let mut nn = Vec::with_capacity(vs.len());
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(dist4(v, u));
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    if nn.len() < 2 {
        return 0.0;
    }
    let mean = nn.iter().sum::<f64>() / nn.len() as f64;
    (nn.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (nn.len() - 1) as f64).sqrt()
}
