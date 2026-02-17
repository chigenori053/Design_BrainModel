use super::{minmax::minmax_scale, robust::robust_standardize};

pub fn normalize_by_depth(values: &[f64], _depth: usize) -> Vec<f64> {
    let robust = robust_standardize(values);
    minmax_scale(&robust, 0.5)
}
