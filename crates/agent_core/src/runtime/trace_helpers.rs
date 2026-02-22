use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use core_types::ObjectiveVector;
use field_engine::{FieldEngine, FieldVector};
use hybrid_vm::{DesignRule, HybridVM, RuleCategory, RuleId, Shm};
use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid, Value};

const FIELD_CACHE_CAPACITY: usize = 50_000;

pub(crate) fn make_dense_trace_chm(shm: &Shm, seed: u64) -> hybrid_vm::Chm {
    let mut chm = HybridVM::empty_chm();
    let ids: Vec<Uuid> = HybridVM::rules(shm).iter().map(|r| r.id).collect();
    for (i, from) in ids.iter().enumerate() {
        for (j, to) in ids.iter().enumerate() {
            if i == j {
                continue;
            }
            HybridVM::chm_insert_edge(&mut chm, *from, *to, pseudo_strength(seed, *from, *to));
        }
    }
    chm
}

fn pseudo_strength(seed: u64, a: Uuid, b: Uuid) -> f64 {
    let mut x = seed ^ (a.as_u128() as u64).wrapping_mul(0x9e3779b97f4a7c15);
    x ^= (b.as_u128() as u64).wrapping_mul(0xD1B54A32D192ED03);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    let frac = (x as f64) / (u64::MAX as f64);
    frac * 2.0 - 1.0
}

pub(crate) fn trace_initial_state(seed: u64) -> DesignState {
    let mut graph = StructuralGraph::default();
    let categories = ["Interface", "Storage", "Network", "Compute", "Control"];
    for i in 0..6u128 {
        let mut attrs = BTreeMap::new();
        attrs.insert("seed".to_string(), Value::Int(seed as i64 + i as i64));
        attrs.insert(
            "category".to_string(),
            Value::Text(categories[(i as usize) % categories.len()].to_string()),
        );
        graph = graph.with_node_added(DesignNode::new(
            Uuid::from_u128(100 + i),
            format!("N{i}"),
            attrs,
        ));
    }
    for i in 0..5u128 {
        graph = graph.with_edge_added(Uuid::from_u128(100 + i), Uuid::from_u128(101 + i));
    }
    DesignState::new(Uuid::from_u128(42), Arc::new(graph), "history:")
}

pub(crate) fn variance(v: &[f64]) -> f64 {
    crate::engine::statistics::variance(v)
}

fn elapsed_us(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000_000.0
}

pub(crate) fn select_k_with_hysteresis(prev_k: usize, stability_index: f64) -> usize {
    match prev_k {
        4 => {
            if stability_index < 0.70 {
                3
            } else {
                4
            }
        }
        3 => {
            if stability_index >= 0.80 {
                4
            } else if stability_index < 0.40 {
                2
            } else {
                3
            }
        }
        _ => {
            if stability_index >= 0.50 {
                3
            } else {
                2
            }
        }
    }
}

pub(crate) fn rule_category_name(category: &RuleCategory) -> &'static str {
    match category {
        RuleCategory::Structural => "Structural",
        RuleCategory::Performance => "Performance",
        RuleCategory::Reliability => "Reliability",
        RuleCategory::Cost => "Cost",
        RuleCategory::Refactor => "Refactor",
        RuleCategory::ConstraintPropagation => "ConstraintPropagation",
    }
}

#[derive(Default)]
pub(crate) struct SoftCandidateBatch {
    pub(crate) candidates: Vec<(DesignState, ObjectiveVector)>,
    pub(crate) depth_category_counts: BTreeMap<String, usize>,
    pub(crate) depth_selected_rules_count: usize,
    pub(crate) field_extract_us: f64,
    pub(crate) field_score_us: f64,
    pub(crate) field_aggregate_us: f64,
    pub(crate) field_total_us: f64,
    pub(crate) chm_us: f64,
}

type FieldCacheKey = (u128, u128, usize, usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct SoftSelectionParams {
    pub(crate) alpha: f64,
    pub(crate) temperature: f64,
    pub(crate) entropy_beta: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct SoftCandidateContext<'a> {
    pub(crate) field: &'a FieldEngine,
    pub(crate) shm: &'a Shm,
    pub(crate) field_profile: bool,
}

pub(crate) fn bounded_cache_get_or_insert(
    cache: &mut BTreeMap<FieldCacheKey, FieldVector>,
    order: &mut VecDeque<FieldCacheKey>,
    key: FieldCacheKey,
    compute: impl FnOnce() -> FieldVector,
) -> (FieldVector, bool) {
    if let Some(found) = cache.get(&key) {
        return (found.clone(), true);
    }
    let value = compute();
    cache.insert(key, value.clone());
    order.push_back(key);
    while cache.len() > FIELD_CACHE_CAPACITY {
        if let Some(old) = order.pop_front() {
            cache.remove(&old);
        } else {
            break;
        }
    }
    (value, false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_soft_candidates_for_frontier(
    vm: &mut HybridVM,
    frontier: &[DesignState],
    beam: usize,
    depth: usize,
    selection: SoftSelectionParams,
    ctx: SoftCandidateContext<'_>,
    field_cache: &mut BTreeMap<FieldCacheKey, FieldVector>,
    field_cache_order: &mut VecDeque<FieldCacheKey>,
) -> SoftCandidateBatch {
    let mut batch = SoftCandidateBatch::default();
    let mut partials: Vec<(DesignState, ObjectiveVector, RuleId, usize, f64)> = Vec::new();

    for (state_idx, state) in frontier.iter().enumerate() {
        let (selected_rules, per_state_counts, _availability_counts) = select_rules_category_soft(
            HybridVM::applicable_rules(ctx.shm, state),
            (beam.max(1) * 5).max(1),
            selection.alpha,
            selection.temperature,
            selection.entropy_beta,
        );
        batch.depth_selected_rules_count += selected_rules.len();
        for (cat, c) in per_state_counts {
            *batch.depth_category_counts.entry(cat).or_insert(0) += c;
        }
        for rule in selected_rules {
            let new_state = crate::apply_atomic(rule, state);
            let obj = vm.evaluate(&new_state);
            let t_chm = Instant::now();
            batch.chm_us += elapsed_us(t_chm);
            let pre_score = 0.4 * obj.f_struct + 0.2 * obj.f_risk + 0.2 * obj.f_shape;
            partials.push((new_state, obj.clamped(), rule.id, state_idx, pre_score));
        }
    }

    partials.sort_by(|(ls, _, _, _, lscore), (rs, _, _, _, rscore)| {
        rscore
            .partial_cmp(lscore)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| ls.id.cmp(&rs.id))
    });

    let detailed_n = (beam.max(1) * 5).min(partials.len());
    for (idx, (state, obj, rule_id, state_idx, _)) in partials.iter_mut().enumerate() {
        if idx >= detailed_n {
            break;
        }
        let t_total = Instant::now();
        let key = (state.id.as_u128(), rule_id.as_u128(), depth, *state_idx);
        let t_extract = Instant::now();
        let t_agg = Instant::now();
        let (_projection, cache_hit) =
            bounded_cache_get_or_insert(field_cache, field_cache_order, key, || {
                ctx.field.aggregate_state(state)
            });
        if ctx.field_profile && !cache_hit {
            batch.field_aggregate_us += elapsed_us(t_agg);
        }
        if ctx.field_profile {
            batch.field_extract_us += elapsed_us(t_extract);
        }
        let t_score = Instant::now();
        if ctx.field_profile {
            batch.field_score_us += elapsed_us(t_score);
            batch.field_total_us += elapsed_us(t_total);
        }
        *obj = obj.clone().clamped();
    }

    batch.candidates = partials.into_iter().map(|(s, o, _, _, _)| (s, o)).collect();
    batch
}

pub(crate) fn select_rules_category_soft(
    rules: Vec<&DesignRule>,
    max_select: usize,
    alpha: f64,
    temperature: f64,
    entropy_beta: f64,
) -> (
    Vec<&DesignRule>,
    BTreeMap<String, usize>,
    BTreeMap<String, usize>,
) {
    if rules.is_empty() {
        return (Vec::new(), BTreeMap::new(), BTreeMap::new());
    }

    let mut availability_counts: BTreeMap<String, usize> = BTreeMap::new();
    for rule in &rules {
        *availability_counts
            .entry(rule_category_name(&rule.category).to_string())
            .or_insert(0) += 1;
    }

    let n_total = rules.len() as f64;
    let k = availability_counts.len().max(1) as f64;
    let uniform = 1.0 / k;
    let entropy = shannon_entropy_from_counts(&availability_counts);
    let t = temperature.max(1e-6);

    let mut scored: Vec<(&DesignRule, f64)> = rules
        .into_iter()
        .map(|rule| {
            let cat = rule_category_name(&rule.category).to_string();
            let p_i = *availability_counts.get(&cat).unwrap_or(&0) as f64 / n_total;
            let w_balance = (-alpha * (p_i - uniform)).exp();
            let s_final = rule.priority * w_balance + entropy_beta * entropy;
            (rule, s_final / t)
        })
        .collect();

    let max_logit = scored
        .iter()
        .map(|(_, logit)| *logit)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut with_prob: Vec<(&DesignRule, f64, f64)> = scored
        .drain(..)
        .map(|(rule, logit)| (rule, logit, (logit - max_logit).exp()))
        .collect();
    let z = with_prob.iter().map(|(_, _, x)| *x).sum::<f64>().max(1e-12);
    for (_, _, x) in &mut with_prob {
        *x /= z;
    }

    with_prob.sort_by(|(l_rule, l_logit, l_prob), (r_rule, r_logit, r_prob)| {
        r_prob
            .partial_cmp(l_prob)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                r_logit
                    .partial_cmp(l_logit)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| l_rule.id.cmp(&r_rule.id))
    });

    let limit = max_select.max(1).min(with_prob.len());
    let mut selected = Vec::with_capacity(limit);
    let mut selected_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (rule, _, _) in with_prob.into_iter().take(limit) {
        selected.push(rule);
        let cat = rule_category_name(&rule.category).to_string();
        *selected_counts.entry(cat).or_insert(0) += 1;
    }

    (selected, selected_counts, availability_counts)
}

pub(crate) fn format_category_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    counts
        .iter()
        .map(|(cat, count)| format!("{cat}:{count}"))
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn shannon_entropy_from_counts(counts: &BTreeMap<String, usize>) -> f64 {
    let total = counts.values().copied().sum::<usize>();
    if total == 0 {
        return 0.0;
    }
    let total_f = total as f64;
    counts
        .values()
        .map(|count| *count as f64 / total_f)
        .filter(|p| *p > 0.0)
        .map(|p| -(p * p.ln()))
        .sum::<f64>()
}

pub(crate) fn update_lambda_entropy(
    lambda: f64,
    entropy: f64,
    target_entropy: f64,
    k: f64,
    ema: f64,
    lambda_min: f64,
    lambda_max: f64,
) -> f64 {
    let e = target_entropy - entropy;
    let lambda_prime = lambda + k * e;
    let lambda_new = (1.0 - ema) * lambda + ema * lambda_prime;
    lambda_new.clamp(lambda_min, lambda_max)
}

pub(crate) fn obj_to_arr(obj: &ObjectiveVector) -> [f64; 4] {
    [obj.f_struct, obj.f_field, obj.f_risk, obj.f_shape]
}

pub(crate) fn arr_to_obj(v: [f64; 4]) -> ObjectiveVector {
    ObjectiveVector {
        f_struct: v[0],
        f_field: v[1],
        f_risk: v[2],
        f_shape: v[3],
    }
}

pub(crate) fn median(v: Vec<f64>) -> f64 {
    crate::engine::statistics::median(v)
}

pub(crate) fn robust_stats_from_samples(
    samples: &[crate::ObjectiveRaw],
    alpha: f64,
) -> Option<crate::GlobalRobustStats> {
    if samples.is_empty() {
        return None;
    }
    let mut med = [0.0; 4];
    let mut mad = [0.0; 4];
    let mut std_dev = [0.0; 4];
    let mut mean = [0.0; 4];
    let mut active = [true; 4];
    let mut weak = [false; 4];
    let mut weights = [0.0; 4];
    let eps_mad = 1e-12;
    let eps_std = 1e-9;
    let alpha_weak = alpha;

    for i in 0..4 {
        let col = samples.iter().map(|v| v.0[i]).collect::<Vec<_>>();
        med[i] = median(col.clone());
        mad[i] = crate::engine::statistics::compute_mad(&col, med[i]);
        mean[i] = crate::engine::statistics::compute_mean(&col);
        std_dev[i] = crate::engine::statistics::compute_std(&col, mean[i]);

        if mad[i] > eps_mad {
            active[i] = true;
            weak[i] = false;
            weights[i] = 1.0;
        } else if std_dev[i] > eps_std {
            active[i] = true;
            weak[i] = true;
            weights[i] = alpha_weak;
        } else {
            active[i] = false;
            weak[i] = false;
            weights[i] = 0.0;
        }
    }
    let mad_zero_count = active
        .iter()
        .zip(mad.iter())
        .filter(|&(&a, &m)| a && m <= eps_mad)
        .count();

    Some(crate::GlobalRobustStats {
        median: med,
        mad,
        mean,
        std: std_dev,
        active_dims: active,
        weak_dims: weak,
        weights,
        mad_zero_count,
        alpha_used: alpha_weak,
    })
}
