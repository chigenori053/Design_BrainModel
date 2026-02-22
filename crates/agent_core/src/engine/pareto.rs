use core_types::ObjectiveVector;
use memory_space::DesignState;

use crate::{DISTANCE_CALL_COUNT, NN_DISTANCE_CALL_COUNT, ObjectiveNorm, ObjectiveRaw};

pub fn dominates(a: &ObjectiveVector, b: &ObjectiveVector) -> bool {
    let all_ge = a.f_struct >= b.f_struct
        && a.f_field >= b.f_field
        && a.f_risk >= b.f_risk
        && a.f_shape >= b.f_shape;
    let one_gt = a.f_struct > b.f_struct
        || a.f_field > b.f_field
        || a.f_risk > b.f_risk
        || a.f_shape > b.f_shape;
    all_ge && one_gt
}

fn median_pairwise_l2(front: &[&ObjectiveVector]) -> f64 {
    if front.len() < 2 {
        return 1e-9;
    }
    let mut dists = Vec::new();
    for i in 0..front.len() {
        for j in (i + 1)..front.len() {
            dists.push(crate::engine::distance::objective_l2_distance(front[i], front[j]));
        }
    }
    crate::engine::statistics::median(dists).max(1e-9)
}

fn objective_energy_distance(a: &ObjectiveVector, b: &ObjectiveVector, tau: f64) -> f64 {
    DISTANCE_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = crate::engine::distance::objective_l2_distance(a, b);
    let t = tau.max(1e-9);
    let k = (-(d * d) / t).exp();
    (1.0 - k).clamp(0.0, 1.0)
}

pub fn pareto_mean_nn_distance(front: &[&ObjectiveVector]) -> f64 {
    if front.len() < 2 {
        return 0.0;
    }
    let tau = median_pairwise_l2(front);
    NN_DISTANCE_CALL_COUNT.fetch_add(
        front.len() * (front.len() - 1),
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut sum = 0.0;
    for (i, obj) in front.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, other) in front.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(objective_energy_distance(obj, other, tau));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / front.len() as f64
}

pub fn pareto_spacing_metric(front: &[&ObjectiveVector]) -> f64 {
    if front.len() < 2 {
        return 0.0;
    }
    let tau = median_pairwise_l2(front);
    let mut nn = Vec::with_capacity(front.len());
    for (i, obj) in front.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, other) in front.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(objective_energy_distance(obj, other, tau));
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    if nn.len() < 2 {
        return 0.0;
    }
    let mean = nn.iter().sum::<f64>() / nn.len() as f64;
    let var = nn.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (nn.len() - 1) as f64;
    var.sqrt()
}

fn hv_2d_rect_approx(points: &[(f64, f64)]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    let mut xs = vec![0.0f64];
    for (x, _) in points {
        xs.push(x.clamp(0.0, 1.0));
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let mut area = 0.0;
    for w in xs.windows(2) {
        let left = w[0];
        let right = w[1];
        if right <= left {
            continue;
        }
        let mid = (left + right) * 0.5;
        let max_y = points
            .iter()
            .filter(|(x, _)| x.clamp(0.0, 1.0) >= mid)
            .map(|(_, y)| y.clamp(0.0, 1.0))
            .fold(0.0f64, f64::max);
        area += (right - left) * max_y;
    }
    area.clamp(0.0, 1.0)
}

pub fn normalize_objective(raw: &ObjectiveRaw, stats: &crate::GlobalRobustStats) -> ObjectiveNorm {
    let eps_small = 1e-9;
    let mut out = [0.0; 4];
    for i in 0..4 {
        if stats.active_dims[i] {
            if !stats.weak_dims[i] {
                let safe_mad = if stats.mad[i] < eps_small {
                    eps_small
                } else {
                    stats.mad[i]
                };
                out[i] = (raw.0[i] - stats.median[i]) / safe_mad;
            } else {
                let den = if stats.std[i] < eps_small {
                    eps_small
                } else {
                    stats.std[i]
                };
                out[i] = (raw.0[i] - stats.mean[i]) / den;
            }
        } else {
            out[i] = 0.0;
        }
    }
    ObjectiveNorm(out)
}

pub fn norm_distance(a: &ObjectiveNorm, b: &ObjectiveNorm, weights: &[f64; 4]) -> f64 {
    DISTANCE_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut s = 0.0;
    let mut w_sum = 0.0;
    for i in 0..4 {
        if weights[i] > 0.0 {
            s += weights[i] * (a.0[i] - b.0[i]).powi(2);
            w_sum += weights[i];
        }
    }
    if w_sum <= 1e-12 {
        return 0.0;
    }
    let dist = (s / w_sum).sqrt();
    let eps_dist = 1e-6;
    if dist < eps_dist { eps_dist } else { dist }
}

pub fn mean_nn_dist_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> f64 {
    if vs.len() < 2 {
        return 0.0;
    }
    NN_DISTANCE_CALL_COUNT.fetch_add(
        vs.len() * (vs.len() - 1),
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut sum = 0.0;
    for (i, v) in vs.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (j, u) in vs.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(norm_distance(v, u, weights));
        }
        if best.is_finite() {
            sum += best;
        }
    }
    sum / vs.len() as f64
}

pub fn count_unique_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> usize {
    let mut unique: Vec<&ObjectiveNorm> = Vec::new();
    for v in vs {
        if !unique.iter().any(|u| norm_distance(v, u, weights) < 1e-6) {
            unique.push(v);
        }
    }
    unique.len()
}

pub fn spacing_norm(vs: &[ObjectiveNorm], weights: &[f64; 4]) -> f64 {
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
            best = best.min(norm_distance(v, u, weights));
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

fn rescale_norm_for_hv(vs: &[ObjectiveNorm]) -> Vec<[f64; 4]> {
    if vs.is_empty() {
        return Vec::new();
    }
    let eps = 1e-6;
    let mut minv = [f64::INFINITY; 4];
    let mut maxv = [f64::NEG_INFINITY; 4];
    for v in vs {
        for i in 0..4 {
            minv[i] = minv[i].min(v.0[i]);
            maxv[i] = maxv[i].max(v.0[i]);
        }
    }
    vs.iter()
        .map(|v| {
            let mut out = [0.0; 4];
            for i in 0..4 {
                out[i] = ((v.0[i] - minv[i]) / (maxv[i] - minv[i] + eps)).clamp(0.0, 1.0);
            }
            out
        })
        .collect()
}

pub fn pareto_hv_2d_norm(vs: &[ObjectiveNorm]) -> f64 {
    let scaled = rescale_norm_for_hv(vs);
    if scaled.is_empty() {
        return 0.0;
    }
    let hv_cost_perf = hv_2d_rect_approx(&scaled.iter().map(|v| (v[3], v[0])).collect::<Vec<_>>());
    let hv_rel_cost = hv_2d_rect_approx(&scaled.iter().map(|v| (v[2], v[3])).collect::<Vec<_>>());
    (hv_cost_perf + hv_rel_cost) * 0.5
}

pub fn select_beam_maxmin_norm(
    front: Vec<(DesignState, ObjectiveVector)>,
    norms: Vec<ObjectiveNorm>,
    beam: usize,
    weights: &[f64; 4],
) -> Vec<(DesignState, ObjectiveVector)> {
    if front.is_empty() {
        return Vec::new();
    }
    let beam = beam.max(1).min(front.len());
    let mut used = vec![false; front.len()];
    let mut selected_idx = Vec::with_capacity(beam);
    let mut seed = 0usize;
    let mut best = f64::NEG_INFINITY;
    for (i, (_, obj)) in front.iter().enumerate() {
        let s = crate::scalar_score(obj);
        if s > best {
            best = s;
            seed = i;
        }
    }
    selected_idx.push(seed);
    used[seed] = true;
    while selected_idx.len() < beam {
        let mut best_i = None;
        let mut best_d = f64::NEG_INFINITY;
        for i in 0..front.len() {
            if used[i] {
                continue;
            }
            let dmin = selected_idx
                .iter()
                .map(|j| norm_distance(&norms[i], &norms[*j], weights))
                .fold(f64::INFINITY, f64::min);
            if dmin > best_d {
                best_d = dmin;
                best_i = Some(i);
            }
        }
        if let Some(i) = best_i {
            used[i] = true;
            selected_idx.push(i);
        } else {
            break;
        }
    }
    selected_idx.into_iter().map(|i| front[i].clone()).collect()
}
