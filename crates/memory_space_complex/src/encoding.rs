use memory_space_core::Complex64;

use crate::ComplexField;

pub fn encode_real_vector(v: &[f64]) -> ComplexField {
    let data = v
        .iter()
        .map(|value| Complex64::new(*value as f32, 0.0))
        .collect();
    ComplexField::new(data)
}
