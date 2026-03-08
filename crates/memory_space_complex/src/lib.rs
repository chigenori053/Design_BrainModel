pub mod algebra;
pub mod complex_field;
pub mod encoding;
pub mod interference;
pub mod normalization;

pub use algebra::{bind, superpose, unbind};
pub use complex_field::ComplexField;
pub use encoding::encode_real_vector;
pub use interference::interfere;
pub use normalization::normalize;
