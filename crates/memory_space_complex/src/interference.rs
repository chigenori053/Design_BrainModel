use crate::{ComplexField, normalize};

pub fn interfere(a: &ComplexField, b: &ComplexField) -> ComplexField {
    let len = a.len().min(b.len());
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        out.push(a.data[idx] * b.data[idx]);
    }

    let mut field = ComplexField::new(out);
    normalize(&mut field);
    field
}
