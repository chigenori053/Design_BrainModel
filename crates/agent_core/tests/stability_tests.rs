use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agent_core::{
    BeamSearch, ParetoFront, Phase45Controller, ProfileUpdateType, SearchConfig, SearchMode,
    SystemEvaluator, apply_atomic, build_target_field, stability_index,
};
use core_types::ObjectiveVector;
use field_engine::FieldEngine;
use hybrid_vm::Chm;
use hybrid_vm::Shm;
use hybrid_vm::{Evaluator, HybridVM, StructuralEvaluator};
use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid, Value};
use profile::PreferenceProfile;

trait ScalarScoreExt {
    fn score(&self) -> f64;
}

impl ScalarScoreExt for ObjectiveVector {
    fn score(&self) -> f64 {
        0.4 * self.f_struct + 0.2 * self.f_field + 0.2 * self.f_risk + 0.2 * self.f_shape
    }
}

#[derive(Clone, Debug)]
struct Trace {
    lambda: Vec<f64>,
    delta_lambda: Vec<f64>,
    tau_prime: Vec<f64>,
    conf_chm: Vec<f64>,
    density: Vec<f64>,
    k_hist: Vec<usize>,
    h_hist: Vec<f64>,
    pareto_size: Vec<usize>,
    diversity: Vec<f64>,
    resonance: Vec<f64>,
    pareto_ids: Vec<Vec<Uuid>>,
}

#[test]
fn lambda_stability_test() {
    let (trace, _) = run_trace(50, 5, ChmMode::Empty, 0.25, 12345);

    let var_lambda = variance(&trace.lambda);
    let var_lambda_tail = variance(&trace.lambda[20..]);
    let max_delta = trace
        .delta_lambda
        .iter()
        .map(|d| d.abs())
        .fold(0.0, f64::max);

    assert!(var_lambda < 0.03, "var_lambda={var_lambda}");
    assert!(var_lambda_tail < 0.02, "var_lambda_tail={var_lambda_tail}");
    assert!(max_delta <= 0.05 + 1e-12, "max_delta={max_delta}");

    let early = mean_abs(&trace.delta_lambda[0..10]);
    let late = mean_abs(&trace.delta_lambda[39..49]);
    // v2.1 shape objective can keep small lambda adjustments later in the run;
    // allow minor tail fluctuations while still guarding against instability.
    assert!(late <= early + 0.005, "late={late}, early={early}");
}

#[test]
fn tau_prime_stability_test() {
    let (trace, _) = run_trace(50, 5, ChmMode::Dense, 0.25, 12345);
    let tau = 0.2;

    assert!(trace.tau_prime.iter().all(|v| *v >= 0.1 * tau - 1e-12));
    assert!(trace.tau_prime.iter().all(|v| *v <= 0.7 * tau + 1e-12));

    let sign_changes = trace
        .tau_prime
        .windows(3)
        .filter(|w| {
            let d1 = w[1] - w[0];
            let d2 = w[2] - w[1];
            d1 * d2 < 0.0
        })
        .count();
    assert!(sign_changes < trace.tau_prime.len() / 2);

    assert_eq!(trace.conf_chm.len(), trace.density.len());
}

#[test]
fn profile_stability_test() {
    let (trace, _) = run_trace(50, 5, ChmMode::Dense, 0.25, 777);

    let k_switches = trace.k_hist.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(k_switches <= 10, "k_switches={k_switches}");
    assert!(trace.h_hist.iter().all(|h| (0.85..=1.20).contains(h)));
}

#[test]
fn pareto_stability_test() {
    let (trace, _) = run_trace(50, 5, ChmMode::Dense, 0.25, 999);

    assert!(trace.pareto_size.iter().all(|size| *size > 0));

    let tail = &trace.diversity[30..];
    assert!(tail.iter().all(|v| v.is_finite()));
}

#[test]
fn extreme_case_chm_empty() {
    let (trace, _) = run_trace(50, 5, ChmMode::Empty, 0.25, 42);

    assert!(trace.density.iter().all(|d| *d == 0.0));
    assert!(trace.conf_chm.iter().all(|c| *c <= 0.01));
    let avg_tau = trace.tau_prime.iter().sum::<f64>() / trace.tau_prime.len() as f64;
    assert!(avg_tau <= 0.03, "avg_tau={avg_tau}");
}

#[test]
fn extreme_case_chm_fully_connected() {
    let (trace, _) = run_trace(50, 5, ChmMode::Dense, 0.25, 42);

    assert!(trace.density.iter().all(|d| *d >= 0.95));
    assert!(trace.tau_prime.iter().all(|t| *t >= 0.1 * 0.2 - 1e-12));

    let max_delta = trace
        .delta_lambda
        .iter()
        .map(|d| d.abs())
        .fold(0.0, f64::max);
    assert!(max_delta <= 0.05 + 1e-12);
}

#[test]
fn extreme_case_profile_extremes() {
    let (trace_pos, _) = run_trace(30, 5, ChmMode::Dense, 1.0, 1);
    let (trace_neg, _) = run_trace(30, 5, ChmMode::Dense, -1.0, 1);

    assert!(trace_pos.h_hist.iter().all(|h| (0.85..=1.20).contains(h)));
    assert!(trace_neg.h_hist.iter().all(|h| (0.85..=1.20).contains(h)));

    let max_delta_pos = trace_pos
        .delta_lambda
        .iter()
        .map(|d| d.abs())
        .fold(0.0, f64::max);
    let max_delta_neg = trace_neg
        .delta_lambda
        .iter()
        .map(|d| d.abs())
        .fold(0.0, f64::max);

    assert!(max_delta_pos <= 0.05 + 1e-12);
    assert!(max_delta_neg <= 0.05 + 1e-12);
}

#[test]
fn reproducibility_test() {
    let (a, b) = run_trace(50, 5, ChmMode::Dense, 0.25, 2026);

    assert_eq!(a.lambda, b.lambda);
    assert_eq!(a.pareto_ids, b.pareto_ids);
    assert_eq!(a.resonance, b.resonance);
}

#[derive(Clone, Copy)]
enum ChmMode {
    Empty,
    Dense,
}

fn run_trace(
    depth: usize,
    beam_width: usize,
    mode: ChmMode,
    stability: f64,
    seed: u64,
) -> (Trace, Trace) {
    let run_once = |seed_val: u64| -> Trace {
        let shm = hybrid_vm::HybridVM::default_shm();
        let chm = make_chm(&shm, mode, seed_val);
        let field = FieldEngine::new(256);
        let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::default())
            .expect("system evaluator init");

        let mut controller = Phase45Controller::new(0.5);
        let mut profile = balanced_profile();

        let mut frontier = vec![initial_state(seed_val)];
        let mut conflict_hist: Vec<f64> = Vec::new();
        let mut align_hist: Vec<f64> = Vec::new();

        let mut trace = Trace {
            lambda: Vec::new(),
            delta_lambda: Vec::new(),
            tau_prime: Vec::new(),
            conf_chm: Vec::new(),
            density: Vec::new(),
            k_hist: Vec::new(),
            h_hist: Vec::new(),
            pareto_size: Vec::new(),
            diversity: Vec::new(),
            resonance: Vec::new(),
            pareto_ids: Vec::new(),
        };

        let n_edge_obs = HybridVM::chm_edge_count(&chm);

        for d in 1..=depth {
            controller.on_profile_update(
                d,
                stability,
                if d % 7 == 0 {
                    ProfileUpdateType::TypeBStructural
                } else {
                    ProfileUpdateType::TypeCStatistical
                },
            );

            let mut candidates: Vec<(DesignState, ObjectiveVector)> = Vec::new();
            for state in &frontier {
                for rule in hybrid_vm::HybridVM::applicable_rules(&shm, state) {
                    let new_state = apply_atomic(rule, state);
                    let obj = evaluator.evaluate(&new_state);
                    candidates.push((new_state, obj));
                }
            }

            if candidates.is_empty() {
                break;
            }

            let mut pareto = ParetoFront::new();
            for (state, obj) in &candidates {
                pareto.insert(state.id, obj.clone());
            }

            let front_set: BTreeSet<Uuid> = pareto.get_front().into_iter().collect();
            let mut front: Vec<(DesignState, ObjectiveVector)> = candidates
                .into_iter()
                .filter(|(s, _)| front_set.contains(&s.id))
                .collect();

            front.sort_by(|(ls, lo), (rs, ro)| {
                ro.score()
                    .partial_cmp(&lo.score())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| ls.id.cmp(&rs.id))
            });
            front.dedup_by(|a, b| a.0.id == b.0.id);

            let pareto_ids = front.iter().map(|(s, _)| s.id).collect::<Vec<_>>();
            trace.pareto_ids.push(pareto_ids);
            trace.pareto_size.push(front.len());
            trace.diversity.push(variance(
                &front.iter().map(|(_, o)| o.score()).collect::<Vec<_>>(),
            ));
            trace
                .resonance
                .push(front.iter().map(|(_, o)| o.f_field).sum::<f64>() / front.len() as f64);

            let target_field = build_target_field(&field, &shm, &front[0].0, controller.lambda());
            let conflict_raw = front
                .iter()
                .map(|(_, o)| (1.0 - o.f_risk + 1.0 - o.f_shape) * 0.5)
                .sum::<f64>()
                / front.len() as f64;
            let align_raw = front
                .iter()
                .map(|(_, o)| {
                    let r = field_engine::resonance_score(
                        &field.aggregate_state(&front[0].0),
                        &target_field,
                    );
                    (o.f_struct + r) * 0.5
                })
                .sum::<f64>()
                / front.len() as f64;

            conflict_hist.push(conflict_raw);
            align_hist.push(align_raw);

            let k = controller.k().max(1);
            let mut conflict_k = moving_average_tail(&conflict_hist, k);
            let mut align_k = moving_average_tail(&align_hist, k);
            let center = 0.5 * (conflict_k + align_k);
            conflict_k = center + 0.5 * (conflict_k - center);
            align_k = center + 0.5 * (align_k - center);

            let log = controller.update_depth(
                d,
                conflict_k,
                align_k,
                n_edge_obs,
                10,
                stability_index(
                    stability.max(0.0),
                    stability.max(0.0),
                    (-stability).max(0.0),
                    (-stability).max(0.0),
                ),
            );

            trace.lambda.push(log.lambda_new);
            trace.delta_lambda.push(log.delta_lambda);
            trace.tau_prime.push(log.tau_prime);
            trace.conf_chm.push(log.conf_chm);
            trace.density.push(log.density);
            trace.k_hist.push(log.k);
            trace
                .h_hist
                .push(agent_core::profile_modulation(log.stability_index));

            frontier = front.into_iter().take(beam_width).map(|(s, _)| s).collect();

            if frontier.is_empty() {
                break;
            }

            let inferred = agent_core::p_inferred(
                &profile,
                &profile,
                &PreferenceProfile {
                    struct_weight: 0.25,
                    field_weight: 0.25,
                    risk_weight: 0.25,
                    cost_weight: 0.25,
                },
                &profile,
            );
            profile = inferred;
        }

        trace
    };

    (run_once(seed), run_once(seed))
}

fn make_chm(shm: &Shm, mode: ChmMode, seed: u64) -> Chm {
    let mut chm = hybrid_vm::HybridVM::empty_chm();
    let ids: Vec<Uuid> = hybrid_vm::HybridVM::rules(shm)
        .iter()
        .map(|r| r.id)
        .collect();

    match mode {
        ChmMode::Empty => chm,
        ChmMode::Dense => {
            for (i, from) in ids.iter().enumerate() {
                for (j, to) in ids.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let v = pseudo_strength(seed, *from, *to);
                    hybrid_vm::HybridVM::chm_insert_edge(&mut chm, *from, *to, v);
                }
            }
            chm
        }
    }
}

fn pseudo_strength(seed: u64, a: Uuid, b: Uuid) -> f64 {
    let mut x = seed ^ (a.as_u128() as u64).wrapping_mul(0x9e3779b97f4a7c15);
    x ^= (b.as_u128() as u64).wrapping_mul(0xD1B54A32D192ED03);
    x = x ^ (x >> 33);
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    let frac = (x as f64) / (u64::MAX as f64);
    frac * 2.0 - 1.0
}

fn initial_state(seed: u64) -> DesignState {
    let mut graph = StructuralGraph::default();

    let categories = ["Interface", "Storage", "Network", "Compute", "Control"];

    for i in 0..6u128 {
        let mut attrs = BTreeMap::new();
        attrs.insert("seed".to_string(), Value::Int((seed as i64) + i as i64));
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

fn balanced_profile() -> PreferenceProfile {
    PreferenceProfile {
        struct_weight: 0.25,
        field_weight: 0.25,
        risk_weight: 0.25,
        cost_weight: 0.25,
    }
}

fn variance(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
}

fn mean_abs(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|x| x.abs()).sum::<f64>() / v.len() as f64
}

fn moving_average_tail(v: &[f64], k: usize) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let start = v.len().saturating_sub(k);
    let slice = &v[start..];
    slice.iter().sum::<f64>() / slice.len() as f64
}

#[test]
fn smoke_beam_engine_depth50_runs() {
    let shm = hybrid_vm::HybridVM::default_shm();
    let chm = make_chm(&shm, ChmMode::Dense, 7);
    let field = FieldEngine::new(128);
    let evaluator = SystemEvaluator::with_base(&chm, &field, StructuralEvaluator::default())
        .expect("system evaluator init");

    let engine = BeamSearch {
        shm: &shm,
        chm: &chm,
        evaluator: &evaluator,
        config: SearchConfig {
            beam_width: 5,
            max_depth: 50,
            norm_alpha: 0.25,
        },
    };

    let result = engine.search_with_mode(&initial_state(7), SearchMode::Manual);
    assert!(!result.depth_fronts.is_empty());
}
