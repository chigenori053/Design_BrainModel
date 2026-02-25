use core_types::ObjectiveVector;
use memory_space::DesignState;

use crate::{DISTANCE_CALL_COUNT, NN_DISTANCE_CALL_COUNT, ObjectiveNorm, ObjectiveRaw};

const HV_EPS: f64 = 1e-12;

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
            dists.push(crate::engine::distance::objective_l2_distance(
                front[i], front[j],
            ));
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

pub fn hv_4d_from_origin_normalized(points: &[[f64; 4]]) -> f64 {
    let mut unique = Vec::<[f64; 4]>::new();
    for p in points {
        let q = [
            p[0].clamp(0.0, 1.0),
            p[1].clamp(0.0, 1.0),
            p[2].clamp(0.0, 1.0),
            p[3].clamp(0.0, 1.0),
        ];
        if !unique
            .iter()
            .any(|u| (0..4).all(|i| (u[i] - q[i]).abs() <= HV_EPS))
        {
            unique.push(q);
        }
    }
    let as_vec = unique
        .iter()
        .map(|p| p.iter().copied().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    hv_recursive_nd(&as_vec, 4)
}

fn hv_recursive_nd(points: &[Vec<f64>], dim: usize) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if dim == 1 {
        return points
            .iter()
            .map(|p| p[0])
            .fold(0.0, |acc, v| if v > acc { v } else { acc });
    }
    let mut coords = points.iter().map(|p| p[0]).collect::<Vec<_>>();
    coords.sort_by(|a, b| a.total_cmp(b));
    coords.dedup_by(|a, b| (*a - *b).abs() <= HV_EPS);

    let mut prev = 0.0;
    let mut volume = 0.0;
    for c in coords {
        let width = c - prev;
        if width > HV_EPS {
            let mut projected = Vec::<Vec<f64>>::new();
            for p in points {
                if p[0] + HV_EPS >= c {
                    projected.push(p[1..dim].to_vec());
                }
            }
            volume += width * hv_recursive_nd(&projected, dim - 1);
        }
        prev = c;
    }
    volume
}

fn dominates_norm(a: &[f64; 4], b: &[f64; 4]) -> bool {
    let all_ge = (0..4).all(|i| a[i] + HV_EPS >= b[i]);
    let one_gt = (0..4).any(|i| a[i] > b[i] + HV_EPS);
    all_ge && one_gt
}

fn pareto_rank_and_domination(points: &[[f64; 4]]) -> (Vec<usize>, Vec<usize>) {
    let n = points.len();
    let mut dominated_by = vec![0usize; n];
    let mut dominates_to: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut rank = vec![0usize; n];
    let mut fronts: Vec<Vec<usize>> = Vec::new();
    let mut first = Vec::new();

    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            if dominates_norm(&points[i], &points[j]) {
                dominates_to[i].push(j);
            } else if dominates_norm(&points[j], &points[i]) {
                dominated_by[i] += 1;
            }
        }
        if dominated_by[i] == 0 {
            rank[i] = 1;
            first.push(i);
        }
    }
    fronts.push(first);
    let mut fi = 0usize;
    while fi < fronts.len() {
        let mut next = Vec::new();
        for &p in &fronts[fi] {
            for &q in &dominates_to[p] {
                if dominated_by[q] > 0 {
                    dominated_by[q] -= 1;
                    if dominated_by[q] == 0 {
                        rank[q] = fi + 2;
                        next.push(q);
                    }
                }
            }
        }
        if !next.is_empty() {
            fronts.push(next);
        }
        fi += 1;
    }
    let domination_count = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && dominates_norm(&points[j], &points[i]))
                .count()
        })
        .collect::<Vec<_>>();
    (rank, domination_count)
}

pub fn select_beam_hv_guided_norm(
    front: Vec<(DesignState, ObjectiveVector)>,
    norms: Vec<ObjectiveNorm>,
    beam: usize,
) -> (Vec<(DesignState, ObjectiveVector)>, f64, f64) {
    if front.is_empty() {
        return (Vec::new(), 0.0, 0.0);
    }
    let beam = beam.max(1).min(front.len());
    let scaled = rescale_norm_for_hv(&norms);
    let (ranks, domination_counts) = pareto_rank_and_domination(&scaled);

    let mut used = vec![false; front.len()];
    let mut selected_idx = Vec::<usize>::with_capacity(beam);
    let mut selected_points = Vec::<[f64; 4]>::new();
    let mut current_hv = 0.0_f64;
    let mut last_delta = 0.0_f64;

    while selected_idx.len() < beam {
        let mut best_idx = None::<usize>;
        let mut best_delta = f64::NEG_INFINITY;
        let mut best_rank = usize::MAX;
        let mut best_dom = usize::MAX;
        let mut best_l2 = f64::NEG_INFINITY;

        for i in 0..front.len() {
            if used[i] {
                continue;
            }
            let dominated = selected_points
                .iter()
                .any(|p| dominates_norm(p, &scaled[i]));
            let hv_next = if dominated {
                current_hv
            } else {
                let mut tmp = selected_points.clone();
                tmp.push(scaled[i]);
                hv_4d_from_origin_normalized(&tmp)
            };
            let delta = (hv_next - current_hv).max(0.0);
            let l2 = (scaled[i][0] * scaled[i][0]
                + scaled[i][1] * scaled[i][1]
                + scaled[i][2] * scaled[i][2]
                + scaled[i][3] * scaled[i][3])
                .sqrt();
            let rank = ranks[i];
            let dom = domination_counts[i];
            let better = delta > best_delta + HV_EPS
                || ((delta - best_delta).abs() <= HV_EPS
                    && (rank < best_rank
                        || (rank == best_rank
                            && (dom < best_dom
                                || (dom == best_dom
                                    && (l2 > best_l2 + HV_EPS
                                        || ((l2 - best_l2).abs() <= HV_EPS
                                            && best_idx.is_none_or(|b| {
                                                front[i].0.id < front[b].0.id
                                            }))))))));
            if better {
                best_idx = Some(i);
                best_delta = delta;
                best_rank = rank;
                best_dom = dom;
                best_l2 = l2;
            }
        }
        let Some(i) = best_idx else {
            break;
        };
        used[i] = true;
        selected_idx.push(i);
        selected_points.push(scaled[i]);
        current_hv = hv_4d_from_origin_normalized(&selected_points);
        last_delta = best_delta.max(0.0);
    }

    (
        selected_idx.into_iter().map(|i| front[i].clone()).collect(),
        current_hv,
        last_delta,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use memory_space::{StateId, StructuralGraph};

    fn mk_state(id: u64) -> DesignState {
        DesignState::new(
            StateId::from_u128(id as u128),
            Arc::new(StructuralGraph::default()),
            "hv-test",
        )
    }

    fn mk_obj(v: [f64; 4]) -> ObjectiveVector {
        ObjectiveVector {
            f_struct: v[0],
            f_field: v[1],
            f_risk: v[2],
            f_shape: v[3],
        }
    }

    #[test]
    fn delta_hv_is_positive_when_frontier_expands() {
        let _ = mk_state(1);
        let _ = mk_obj([0.2, 0.2, 0.2, 0.2]);
        let f = [[0.2, 0.2, 0.2, 0.2]];
        let f_ext = [[0.2, 0.2, 0.2, 0.2], [0.8, 0.2, 0.2, 0.2]];
        let delta = hv_4d_from_origin_normalized(&f_ext) - hv_4d_from_origin_normalized(&f);
        assert!(delta > 0.0);
    }

    #[test]
    fn dominated_candidate_has_zero_delta_hv() {
        let points = [[0.8, 0.8, 0.8, 0.8], [0.2, 0.2, 0.2, 0.2]];
        let hv1 = hv_4d_from_origin_normalized(&[points[0]]);
        let hv2 = hv_4d_from_origin_normalized(&points);
        assert!((hv2 - hv1).abs() <= 1e-12);
    }

    #[test]
    fn hypervolume_is_monotonic_non_decreasing() {
        let f1 = [[0.2, 0.2, 0.2, 0.2]];
        let f2 = [[0.2, 0.2, 0.2, 0.2], [0.8, 0.2, 0.2, 0.2]];
        let hv1 = hv_4d_from_origin_normalized(&f1);
        let hv2 = hv_4d_from_origin_normalized(&f2);
        assert!(hv2 + 1e-12 >= hv1);
    }
}
