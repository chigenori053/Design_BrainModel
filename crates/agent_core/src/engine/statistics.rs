pub fn compute_mad(values: &[f64], med: f64) -> f64 {
    let diffs: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    median(diffs)
}

pub fn compute_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

pub fn compute_std(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
}

#[allow(dead_code)]
pub fn moving_average_tail(values: &[f64], k: usize) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let start = values.len().saturating_sub(k);
    let slice = &values[start..];
    slice.iter().sum::<f64>() / slice.len() as f64
}

pub fn variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64
}

pub fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}
