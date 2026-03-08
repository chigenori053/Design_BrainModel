use memory_space_complex::ComplexField;

use crate::field::ConceptField;

pub fn similarity(lhs: &ConceptField, rhs: &ComplexField) -> f64 {
    if lhs.vector.data.is_empty() || rhs.data.is_empty() {
        return 0.0;
    }

    let len = lhs.vector.data.len().min(rhs.data.len());
    let mut dot = 0.0f64;
    let mut nl = 0.0f64;
    let mut nr = 0.0f64;

    for i in 0..len {
        dot += f64::from((lhs.vector.data[i] * rhs.data[i].conj()).re);
        nl += f64::from(lhs.vector.data[i].norm_sqr());
        nr += f64::from(rhs.data[i].norm_sqr());
    }

    if nl <= f64::EPSILON || nr <= f64::EPSILON {
        return 0.0;
    }

    (dot / (nl.sqrt() * nr.sqrt())).clamp(-1.0, 1.0)
}
