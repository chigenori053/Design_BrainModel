use memory_space_complex::ComplexField;
use memory_space_core::Complex64;

pub fn resonance(query: &ComplexField, memory: &ComplexField) -> f64 {
    let len = query.data.len().min(memory.data.len());
    if len == 0 {
        return 0.0;
    }

    let mut dot = Complex64::new(0.0, 0.0);
    for idx in 0..len {
        let q = query.data[idx];
        let m_conj = memory.data[idx].conj();
        dot += q * m_conj;
    }

    (dot.norm() as f64).clamp(0.0, 1.0)
}
