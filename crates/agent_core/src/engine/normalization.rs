use core_types::ObjectiveVector;
use memory_space::DesignState;

use crate::GlobalRobustStats;

pub const EPSILON_JITTER: f64 = 1e-6;
const ZSCORE_EPSILON: f64 = 1e-6;
const STD_THRESHOLD: f64 = 1e-6;
const ZSCORE_CLIP: f64 = 3.0;
const CORRELATION_THRESHOLD: f64 = 0.8;
const NORMALIZED_MARGIN: f64 = 1e-3;

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
    let mut matrix = Vec::<[f64; 4]>::with_capacity(n);
    for (_, obj) in &candidates {
        matrix.push([obj.f_struct, obj.f_field, obj.f_risk, obj.f_shape]);
    }
    decorrelate_high_correlation(&mut matrix);

    let structs = matrix.iter().map(|row| row[0]).collect::<Vec<_>>();
    let fields = matrix.iter().map(|row| row[1]).collect::<Vec<_>>();
    let risks = matrix.iter().map(|row| row[2]).collect::<Vec<_>>();
    let costs = matrix.iter().map(|row| row[3]).collect::<Vec<_>>();

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

    let active = [
        std_s >= STD_THRESHOLD,
        std_f >= STD_THRESHOLD,
        std_r >= STD_THRESHOLD,
        std_c >= STD_THRESHOLD,
    ];
    let weak = [false; 4];
    let weights = [1.0; 4];
    let mad = [mad_s, mad_f, mad_r, mad_c];
    let median = [med_s, med_f, med_r, med_c];
    let mean = [mean_s, mean_f, mean_r, mean_c];
    let std_dev = [std_s, std_f, std_r, std_c];
    let mad_zero_count = std_dev.iter().filter(|&&s| s < STD_THRESHOLD).count();

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

    let normalized = candidates
        .into_iter()
        .zip(matrix)
        .map(|((state, _), row)| {
            let out = [
                zscore_to_unit(row[0], mean_s, std_s),
                zscore_to_unit(row[1], mean_f, std_f),
                zscore_to_unit(row[2], mean_r, std_r),
                zscore_to_unit(row[3], mean_c, std_c),
            ];
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

fn zscore_to_unit(value: f64, mean: f64, std: f64) -> f64 {
    if !value.is_finite() {
        return 0.5;
    }
    let z = if std < STD_THRESHOLD {
        0.0
    } else {
        ((value - mean) / (std + ZSCORE_EPSILON)).clamp(-ZSCORE_CLIP, ZSCORE_CLIP)
    };
    let base = ((z + ZSCORE_CLIP) / (2.0 * ZSCORE_CLIP)).clamp(0.0, 1.0);
    (NORMALIZED_MARGIN + (1.0 - 2.0 * NORMALIZED_MARGIN) * base).clamp(0.0, 1.0)
}

fn decorrelate_high_correlation(matrix: &mut [[f64; 4]]) {
    for anchor in 0..4 {
        for target in (anchor + 1)..4 {
            let corr = pearson_corr(matrix, anchor, target);
            if corr.abs() <= CORRELATION_THRESHOLD {
                continue;
            }
            let mean_anchor = column_mean(matrix, anchor);
            let mean_target = column_mean(matrix, target);
            let var_anchor = column_variance(matrix, anchor, mean_anchor);
            if var_anchor < STD_THRESHOLD {
                continue;
            }
            let covariance = matrix
                .iter()
                .map(|row| (row[anchor] - mean_anchor) * (row[target] - mean_target))
                .sum::<f64>()
                / matrix.len() as f64;
            let beta = covariance / var_anchor;
            for row in matrix.iter_mut() {
                row[target] = (row[target] - mean_target) - beta * (row[anchor] - mean_anchor);
            }
        }
    }
}

fn column_mean(matrix: &[[f64; 4]], idx: usize) -> f64 {
    if matrix.is_empty() {
        return 0.0;
    }
    matrix.iter().map(|row| row[idx]).sum::<f64>() / matrix.len() as f64
}

fn column_variance(matrix: &[[f64; 4]], idx: usize, mean: f64) -> f64 {
    if matrix.len() < 2 {
        return 0.0;
    }
    matrix
        .iter()
        .map(|row| {
            let delta = row[idx] - mean;
            delta * delta
        })
        .sum::<f64>()
        / matrix.len() as f64
}

fn pearson_corr(matrix: &[[f64; 4]], a: usize, b: usize) -> f64 {
    if matrix.len() < 2 {
        return 0.0;
    }
    let mean_a = column_mean(matrix, a);
    let mean_b = column_mean(matrix, b);
    let var_a = column_variance(matrix, a, mean_a);
    let var_b = column_variance(matrix, b, mean_b);
    if var_a < STD_THRESHOLD || var_b < STD_THRESHOLD {
        return 0.0;
    }
    let covariance = matrix
        .iter()
        .map(|row| (row[a] - mean_a) * (row[b] - mean_b))
        .sum::<f64>()
        / matrix.len() as f64;
    (covariance / ((var_a.sqrt() * var_b.sqrt()) + ZSCORE_EPSILON)).clamp(-1.0, 1.0)
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
