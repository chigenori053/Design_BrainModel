use design_reasoning::{
    DesignFactor, FactorType, ScsInputs, compute_dependency_consistency_metrics, compute_scs_v1_1,
    sanitize_factors,
};

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

        let normalized_depth = normalize_phase1_vectors(
            &candidates
                .iter()
                .map(|(_, o, _)| o.clone())
                .collect::<Vec<_>>(),
        );
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
        let front_objs = front_entries
            .iter()
            .map(|(_, o, _)| o.clone())
            .collect::<Vec<_>>();
        let mut order: Vec<usize> = (0..front_entries.len()).collect();
        if matches!(config.hv_policy, crate::HvPolicy::Legacy) {
            order.sort_by(|&li, &ri| {
                crate::scalar_score(&front_objs[ri])
                    .partial_cmp(&crate::scalar_score(&front_objs[li]))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| front_entries[li].0.id.cmp(&front_entries[ri].0.id))
            });
        } else {
            let scores = crate::engine::normalization::soft_dominance_scores(
                &front_objs,
                crate::SOFT_PARETO_TEMPERATURE,
            );
            order.sort_by(|&li, &ri| {
                scores[ri]
                    .partial_cmp(&scores[li])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        crate::scalar_score(&front_objs[ri])
                            .partial_cmp(&crate::scalar_score(&front_objs[li]))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| front_entries[li].0.id.cmp(&front_entries[ri].0.id))
            });
        }
        let front: Vec<(
            memory_space::DesignState,
            core_types::ObjectiveVector,
            memory_space::Uuid,
        )> = order
            .into_iter()
            .map(|idx| front_entries[idx].clone())
            .collect();

        let normalized_front =
            normalize_phase1_vectors(&front.iter().map(|(_, o, _)| o.clone()).collect::<Vec<_>>());
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
            collapse_flag,
        });

        let beam_take = config.beam_width.max(1).min(front.len());
        let mut id_to_norm: std::collections::BTreeMap<u128, [f64; 4]> =
            std::collections::BTreeMap::new();
        for ((state, _, _), norm) in front.iter().zip(normalized_front.iter()) {
            id_to_norm.insert(state.id.as_u128(), *norm);
        }
        for (beam_index, (state, obj, rid)) in front.iter().take(beam_take).enumerate() {
            let norm = id_to_norm
                .get(&state.id.as_u128())
                .copied()
                .unwrap_or([0.0; 4]);
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
                rule_id: format!("{:032x}", rid.as_u128()),
                objective_vector_raw: fmt_vec4(&crate::runtime::trace_helpers::obj_to_arr(obj)),
                objective_vector_norm: fmt_vec4(&norm),
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
            let select_front = front
                .iter()
                .map(|(s, o, _)| (s.clone(), o.clone()))
                .collect::<Vec<_>>();
            let select_norms = normalized_front
                .iter()
                .map(|v| crate::ObjectiveNorm(*v))
                .collect::<Vec<_>>();
            let (selected, current_hv, delta_hv_selected) =
                crate::engine::pareto::select_beam_hv_guided_norm(
                    select_front,
                    select_norms,
                    beam_take,
                );
            eprintln!(
                "hv_guided iteration={} current_HV={:.8} delta_HV_selected={:.8} frontier_size={}",
                depth,
                current_hv,
                delta_hv_selected,
                selected.len()
            );
            delta_hv_window.push_back(delta_hv_selected);
            if delta_hv_window.len() > HV_STOP_WINDOW {
                delta_hv_window.pop_front();
            }
            frontier = selected.into_iter().map(|(s, _)| s).collect();
        } else {
            frontier = front
                .into_iter()
                .take(beam_take)
                .map(|(s, _, _)| s)
                .collect();
        }
        if frontier.is_empty() {
            frontier = vec![crate::runtime::trace_helpers::trace_initial_state(
                config.seed,
            )];
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

fn normalize_phase1_vectors(objs: &[core_types::ObjectiveVector]) -> Vec<[f64; 4]> {
    if objs.is_empty() {
        return Vec::new();
    }
    let eps = 1e-6;
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
                out[i] = (v[i] - meds[i]) / (mads[i] + eps);
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
