use memory_space_core::Complex64;

#[derive(Clone, Debug, PartialEq)]
pub struct ComplexField {
    pub data: Vec<Complex64>,
}

impl ComplexField {
    pub fn new(data: Vec<Complex64>) -> Self {
        Self { data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}
