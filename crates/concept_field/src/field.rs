use memory_space_complex::ComplexField;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldConfig {
    pub coarse_dim: usize,
    pub medium_dim: usize,
    pub reasoning_dim: usize,
}

impl Default for FieldConfig {
    fn default() -> Self {
        Self {
            coarse_dim: 0,
            medium_dim: 0,
            reasoning_dim: 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConceptField {
    pub vector: ComplexField,
    pub config: FieldConfig,
}
