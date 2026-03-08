use memory_space_core::Complex64;

use crate::{ComplexField, normalize};

pub fn bind(a: &ComplexField, b: &ComplexField) -> ComplexField {
    let len = a.data.len().min(b.data.len());
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        out.push(a.data[idx] * b.data[idx]);
    }

    let mut field = ComplexField::new(out);
    normalize(&mut field);
    field
}

pub fn unbind(bound: &ComplexField, key: &ComplexField) -> ComplexField {
    let len = bound.data.len().min(key.data.len());
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        out.push(bound.data[idx] * key.data[idx].conj());
    }

    let mut field = ComplexField::new(out);
    normalize(&mut field);
    field
}

pub fn superpose(memories: &[ComplexField]) -> ComplexField {
    if memories.is_empty() {
        return ComplexField::new(Vec::new());
    }

    let len = memories.iter().map(|m| m.data.len()).min().unwrap_or(0);
    let mut out = vec![Complex64::new(0.0, 0.0); len];

    for memory in memories {
        for (idx, value) in memory.data.iter().take(len).enumerate() {
            out[idx] += *value;
        }
    }

    let mut field = ComplexField::new(out);
    normalize(&mut field);
    field
}

#[cfg(test)]
mod tests {
    use memory_space_core::Complex64;

    use crate::{ComplexField, bind, normalize, superpose, unbind};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn cosine_similarity(a: &ComplexField, b: &ComplexField) -> f64 {
        let len = a.data.len().min(b.data.len());
        if len == 0 {
            return 0.0;
        }

        let mut dot = Complex64::new(0.0, 0.0);
        let mut a_norm = 0.0f64;
        let mut b_norm = 0.0f64;

        for idx in 0..len {
            let av = a.data[idx];
            let bv = b.data[idx];
            dot += av * bv.conj();
            a_norm += av.norm_sqr() as f64;
            b_norm += bv.norm_sqr() as f64;
        }

        let denom = (a_norm.sqrt() * b_norm.sqrt()).max(f64::EPSILON);
        (dot.norm() as f64 / denom).clamp(0.0, 1.0)
    }

    #[test]
    fn bind_unbind_inverse_direction() {
        let mut source = ComplexField::new(vec![
            Complex64::new(0.4, 0.0),
            Complex64::new(0.1, 0.3),
            Complex64::new(-0.2, 0.5),
        ]);
        normalize(&mut source);

        let mut key = ComplexField::new(vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(0.0, 1.0),
            Complex64::new(-1.0, 0.0),
        ]);
        normalize(&mut key);

        let bound = bind(&source, &key);
        let recovered = unbind(&bound, &key);
        let similarity = cosine_similarity(&source, &recovered);

        assert!(similarity > 0.999);
    }

    #[test]
    fn superpose_is_stable() {
        let mut a = ComplexField::new(vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 1.0)]);
        let mut b = ComplexField::new(vec![Complex64::new(0.5, 0.5), Complex64::new(-0.5, 0.5)]);
        normalize(&mut a);
        normalize(&mut b);

        let s1 = superpose(&[a.clone(), b.clone()]);
        let s2 = superpose(&[a, b]);

        for idx in 0..s1.data.len() {
            assert!(approx_eq(s1.data[idx].re, s2.data[idx].re, 1e-6));
            assert!(approx_eq(s1.data[idx].im, s2.data[idx].im, 1e-6));
        }
    }
}
