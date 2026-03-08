use memory_space_complex::{ComplexField, encode_real_vector, normalize};

pub fn perceive_from_vector(values: &[f64]) -> ComplexField {
    let mut field = encode_real_vector(values);
    normalize(&mut field);
    field
}

pub fn perceive(input: &ComplexField) -> ComplexField {
    let mut field = input.clone();
    normalize(&mut field);
    field
}
