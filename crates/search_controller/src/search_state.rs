use memory_space_complex::ComplexField;

#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    pub state_vector: ComplexField,
    pub score: f64,
    pub depth: usize,
}
