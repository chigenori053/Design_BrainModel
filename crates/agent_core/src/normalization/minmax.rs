pub fn minmax_scale(values: &[f64], empty_value: f64) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for &v in values {
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }

    let range = max_v - min_v;
    if !range.is_finite() || range.abs() <= 1e-12 {
        return vec![empty_value; values.len()];
    }

    values
        .iter()
        .map(|v| ((v - min_v) / range).clamp(0.0, 1.0))
        .collect()
}
