use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const EPS: f64 = 1e-12;
const HIST_BINS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseData {
    pub objective: [f64; 4],
    pub sc: f64,
    pub frontier: bool,
    pub pareto_rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistogramBin {
    pub lower: f64,
    pub upper: f64,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationReport {
    pub sample_size: usize,
    pub corr_hc_o1: f64,
    pub corr_hc_o2: f64,
    pub corr_hc_o3: f64,
    pub corr_hc_o4: f64,
    pub corr_hc_total: f64,
    pub corr_hc_pareto_rank: f64,
    pub mean_hc_frontier: f64,
    pub mean_hc_non_frontier: f64,
    pub delta_hc_frontier_non_frontier: f64,
    pub hc_mean_all: f64,
    pub hc_stddev_all: f64,
    pub hc_min_all: f64,
    pub hc_max_all: f64,
    pub mean_hc_by_rank: BTreeMap<usize, f64>,
    pub hc_frontier_share_top10pct: f64,
    pub hc_frontier_share_top20pct: f64,
    pub hc_histogram: Vec<HistogramBin>,
}

pub fn compute_correlation(data: &[CaseData]) -> CorrelationReport {
    let sc = data.iter().map(|c| c.sc).collect::<Vec<_>>();
    let o1 = data.iter().map(|c| c.objective[0]).collect::<Vec<_>>();
    let o2 = data.iter().map(|c| c.objective[1]).collect::<Vec<_>>();
    let o3 = data.iter().map(|c| c.objective[2]).collect::<Vec<_>>();
    let o4 = data.iter().map(|c| c.objective[3]).collect::<Vec<_>>();
    let total = data
        .iter()
        .map(|c| c.objective.iter().sum::<f64>() / 4.0)
        .collect::<Vec<_>>();
    let ranks = data
        .iter()
        .map(|c| c.pareto_rank as f64)
        .collect::<Vec<_>>();

    let (frontier_sum, frontier_count) = data.iter().fold((0.0, 0usize), |(s, n), c| {
        if c.frontier {
            (s + c.sc, n + 1)
        } else {
            (s, n)
        }
    });
    let (non_frontier_sum, non_frontier_count) = data.iter().fold((0.0, 0usize), |(s, n), c| {
        if c.frontier {
            (s, n)
        } else {
            (s + c.sc, n + 1)
        }
    });

    CorrelationReport {
        sample_size: data.len(),
        corr_hc_o1: pearson(&sc, &o1),
        corr_hc_o2: pearson(&sc, &o2),
        corr_hc_o3: pearson(&sc, &o3),
        corr_hc_o4: pearson(&sc, &o4),
        corr_hc_total: pearson(&sc, &total),
        corr_hc_pareto_rank: pearson(&sc, &ranks),
        mean_hc_frontier: safe_mean(frontier_sum, frontier_count),
        mean_hc_non_frontier: safe_mean(non_frontier_sum, non_frontier_count),
        delta_hc_frontier_non_frontier: round6(
            safe_mean(frontier_sum, frontier_count)
                - safe_mean(non_frontier_sum, non_frontier_count),
        ),
        hc_mean_all: round6(mean(&sc)),
        hc_stddev_all: round6(stddev(&sc)),
        hc_min_all: round6(min_value(&sc)),
        hc_max_all: round6(max_value(&sc)),
        mean_hc_by_rank: mean_by_rank(data),
        hc_frontier_share_top10pct: frontier_share_top_pct(data, 0.10),
        hc_frontier_share_top20pct: frontier_share_top_pct(data, 0.20),
        hc_histogram: histogram(&sc),
    }
}

fn safe_mean(sum: f64, n: usize) -> f64 {
    if n == 0 { 0.0 } else { round6(sum / n as f64) }
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for (ai, bi) in a.iter().zip(b.iter()) {
        let da = *ai - mean_a;
        let db = *bi - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    let denom = var_a.sqrt() * var_b.sqrt();
    if denom <= EPS {
        0.0
    } else {
        round6((cov / denom).clamp(-1.0, 1.0))
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn stddev(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let m = mean(values);
    let var = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt()
}

fn min_value(values: &[f64]) -> f64 {
    values.iter().copied().reduce(f64::min).unwrap_or(0.0)
}

fn max_value(values: &[f64]) -> f64 {
    values.iter().copied().reduce(f64::max).unwrap_or(0.0)
}

fn mean_by_rank(data: &[CaseData]) -> BTreeMap<usize, f64> {
    let mut sum = BTreeMap::<usize, f64>::new();
    let mut n = BTreeMap::<usize, usize>::new();
    for c in data {
        *sum.entry(c.pareto_rank).or_insert(0.0) += c.sc;
        *n.entry(c.pareto_rank).or_insert(0) += 1;
    }
    let mut out = BTreeMap::<usize, f64>::new();
    for (rank, s) in sum {
        let count = n.get(&rank).copied().unwrap_or(1);
        out.insert(rank, round6(s / count as f64));
    }
    out
}

fn frontier_share_top_pct(data: &[CaseData], pct: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut v = data.to_vec();
    v.sort_by(|a, b| b.sc.total_cmp(&a.sc));
    let top_n = ((data.len() as f64 * pct).ceil() as usize)
        .max(1)
        .min(data.len());
    let frontier_count = v.iter().take(top_n).filter(|c| c.frontier).count();
    round6(frontier_count as f64 / top_n as f64)
}

fn histogram(values: &[f64]) -> Vec<HistogramBin> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut bins = vec![0usize; HIST_BINS];
    for v in values {
        let clamped = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let idx = (clamped * HIST_BINS as f64).floor() as usize;
        bins[idx.min(HIST_BINS - 1)] += 1;
    }
    bins.into_iter()
        .enumerate()
        .map(|(i, count)| {
            let lower = i as f64 / HIST_BINS as f64;
            let upper = (i + 1) as f64 / HIST_BINS as f64;
            HistogramBin {
                lower: round6(lower),
                upper: round6(upper),
                count,
            }
        })
        .collect()
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_deterministic() {
        let data = vec![
            CaseData {
                objective: [0.1, 0.2, 0.3, 0.4],
                sc: 0.2,
                frontier: false,
                pareto_rank: 2,
            },
            CaseData {
                objective: [0.2, 0.3, 0.4, 0.5],
                sc: 0.3,
                frontier: true,
                pareto_rank: 1,
            },
            CaseData {
                objective: [0.3, 0.4, 0.5, 0.6],
                sc: 0.4,
                frontier: true,
                pareto_rank: 1,
            },
        ];
        let a = compute_correlation(&data);
        let b = compute_correlation(&data);
        assert_eq!(a.sample_size, 3);
        assert_eq!(a.corr_hc_o1, b.corr_hc_o1);
        assert_eq!(a.mean_hc_frontier, b.mean_hc_frontier);
        assert_eq!(a.hc_histogram, b.hc_histogram);
        assert!(a.hc_stddev_all >= 0.0);
        assert!(a.hc_min_all <= a.hc_max_all);
    }
}
