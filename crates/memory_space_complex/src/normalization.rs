use crate::ComplexField;

pub fn normalize(field: &mut ComplexField) {
    let norm = field
        .data
        .iter()
        .map(|z| z.norm_sqr() as f64)
        .sum::<f64>()
        .sqrt();

    if norm <= f64::EPSILON {
        return;
    }

    let inv = (1.0 / norm) as f32;
    for z in &mut field.data {
        *z = *z * inv;
    }
}
