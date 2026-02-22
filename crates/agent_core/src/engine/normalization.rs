use core_types::ObjectiveVector;
use memory_space::DesignState;

use crate::GlobalRobustStats;
use crate::normalization;

pub const EPSILON_JITTER: f64 = 1e-6;

pub fn epsilon_jitter(value: f64, state_id: u64, idx: u64) -> f64 {
    let mut x = state_id ^ idx.wrapping_mul(0x9e3779b97f4a7c15);
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 32;
    let noise = (x as f64 / u64::MAX as f64) - 0.5;
    value + EPSILON_JITTER * noise
}

pub fn objective_distance(a: &ObjectiveVector, b: &ObjectiveVector) -> f64 {
    let ds = a.f_struct - b.f_struct;
    let df = a.f_field - b.f_field;
    let dr = a.f_risk - b.f_risk;
    let dc = a.f_shape - b.f_shape;
    (ds * ds + df * df + dr * dr + dc * dc).sqrt()
}

pub fn soft_sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        let z = (-x).exp();
        1.0 / (1.0 + z)
    } else {
        let z = x.exp();
        z / (1.0 + z)
    }
}

pub fn dominance_probability(a: &ObjectiveVector, b: &ObjectiveVector, temperature: f64) -> f64 {
    let t = temperature.max(1e-9);
    let diffs = [
        a.f_struct - b.f_struct,
        a.f_field - b.f_field,
        a.f_risk - b.f_risk,
        a.f_shape - b.f_shape,
    ];
    diffs
        .into_iter()
        .map(|d| soft_sigmoid(d / t))
        .product::<f64>()
}

pub fn soft_dominance_scores(objs: &[ObjectiveVector], temperature: f64) -> Vec<f64> {
    let n = objs.len();
    let mut scores = vec![0.0f64; n];
    for i in 0..n {
        let mut score = 0.0;
        for j in 0..n {
            if i == j {
                continue;
            }
            score += dominance_probability(&objs[i], &objs[j], temperature);
        }
        scores[i] = score;
    }
    scores
}

pub fn normalize_by_depth_candidates(
    candidates: Vec<(DesignState, ObjectiveVector)>,
    alpha: f64,
) -> (Vec<(DesignState, ObjectiveVector)>, GlobalRobustStats) {
    if candidates.is_empty() {
        return (
            Vec::new(),
            GlobalRobustStats {
                median: [0.0; 4],
                mad: [1.0; 4],
                mean: [0.0; 4],
                std: [1.0; 4],
                active_dims: [true; 4],
                weak_dims: [false; 4],
                weights: [1.0; 4],
                mad_zero_count: 0,
                alpha_used: alpha,
            },
        );
    }

    let candidates = candidates
        .into_iter()
        .map(|(state, obj)| {
            let sid = state_id_to_u64(state.id.as_u128());
            let jittered = ObjectiveVector {
                f_struct: epsilon_jitter(obj.f_struct, sid, 0),
                f_field: epsilon_jitter(obj.f_field, sid, 1),
                f_risk: epsilon_jitter(obj.f_risk, sid, 2),
                f_shape: epsilon_jitter(obj.f_shape, sid, 3),
            };
            (state, jittered)
        })
        .collect::<Vec<_>>();

    let n = candidates.len();
    let mut structs = Vec::with_capacity(n);
    let mut fields = Vec::with_capacity(n);
    let mut risks = Vec::with_capacity(n);
    let mut costs = Vec::with_capacity(n);

    for (_, obj) in &candidates {
        structs.push(obj.f_struct);
        fields.push(obj.f_field);
        risks.push(obj.f_risk);
        costs.push(obj.f_shape);
    }

    let med_s = median(structs.clone());
    let med_f = median(fields.clone());
    let med_r = median(risks.clone());
    let med_c = median(costs.clone());

    let mad_s = compute_mad(&structs, med_s);
    let mad_f = compute_mad(&fields, med_f);
    let mad_r = compute_mad(&risks, med_r);
    let mad_c = compute_mad(&costs, med_c);

    let mean_s = compute_mean(&structs);
    let mean_f = compute_mean(&fields);
    let mean_r = compute_mean(&risks);
    let mean_c = compute_mean(&costs);

    let std_s = compute_std(&structs, mean_s);
    let std_f = compute_std(&fields, mean_f);
    let std_r = compute_std(&risks, mean_r);
    let std_c = compute_std(&costs, mean_c);

    let eps_mad = 1e-12;
    let active = [true; 4];
    let weak = [false; 4];
    let weights = [1.0; 4];
    let mad = [mad_s, mad_f, mad_r, mad_c];
    let median = [med_s, med_f, med_r, med_c];
    let mean = [mean_s, mean_f, mean_r, mean_c];
    let std_dev = [std_s, std_f, std_r, std_c];
    let mad_zero_count = mad.iter().filter(|&&m| m <= eps_mad).count();

    let stats = GlobalRobustStats {
        median,
        mad,
        mean,
        std: std_dev,
        active_dims: active,
        weak_dims: weak,
        weights,
        mad_zero_count,
        alpha_used: alpha,
    };

    let normalized_raw = candidates
        .into_iter()
        .map(|(state, obj)| {
            let mut f = [0.0; 4];
            let raw = [obj.f_struct, obj.f_field, obj.f_risk, obj.f_shape];
            for i in 0..4 {
                f[i] = (raw[i] - median[i]) / (mad[i] + eps_mad);
            }
            (state, f)
        })
        .collect::<Vec<_>>();

    let mut dim_values = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (_, vals) in &normalized_raw {
        for i in 0..4 {
            dim_values[i].push(vals[i]);
        }
    }
    let dim_scaled = [
        normalization::depth::normalize_by_depth(&dim_values[0], 0),
        normalization::depth::normalize_by_depth(&dim_values[1], 0),
        normalization::depth::normalize_by_depth(&dim_values[2], 0),
        normalization::depth::normalize_by_depth(&dim_values[3], 0),
    ];

    let normalized = normalized_raw
        .into_iter()
        .enumerate()
        .map(|(idx, (state, _))| {
            let mut out = [0.0; 4];
            for i in 0..4 {
                out[i] = if idx < dim_scaled[i].len() && dim_scaled[i][idx].is_finite() {
                    dim_scaled[i][idx].clamp(0.0, 1.0)
                } else {
                    0.5
                };
            }
            (
                state,
                ObjectiveVector {
                    f_struct: out[0],
                    f_field: out[1],
                    f_risk: out[2],
                    f_shape: out[3],
                },
            )
        })
        .collect::<Vec<_>>();

    (normalized, stats)
}

fn state_id_to_u64(raw: u128) -> u64 {
    (raw as u64) ^ ((raw >> 64) as u64)
}

fn compute_mad(values: &[f64], med: f64) -> f64 {
    let diffs: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    median(diffs)
}

fn compute_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn compute_std(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        0.5 * (v[mid - 1] + v[mid])
    }
}
