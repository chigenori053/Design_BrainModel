use num_complex::Complex;

pub type MemoryId = u64;
pub type Complex64 = Complex<f32>;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryField {
    pub id: MemoryId,
    pub vector: Vec<Complex64>,
}

impl MemoryField {
    pub fn dimension(&self) -> usize {
        self.vector.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vector.is_empty()
    }
}
