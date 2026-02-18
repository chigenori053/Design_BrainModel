pub fn robust_standardize(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    let median = median(values.to_vec());
    let mad = median_absolute_deviation(values, median).max(1e-12);
    values.iter().map(|v| (v - median) / mad).collect()
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) * 0.5
    } else {
        values[n / 2]
    }
}

fn median_absolute_deviation(values: &[f64], median: f64) -> f64 {
    let mut diffs = values
        .iter()
        .map(|v| (v - median).abs())
        .collect::<Vec<_>>();
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = diffs.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        (diffs[n / 2 - 1] + diffs[n / 2]) * 0.5
    } else {
        diffs[n / 2]
    }
}
