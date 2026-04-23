pub mod error;
pub mod generator;
pub mod ir;
pub mod spec;

pub use error::CodegenError;
pub use generator::generate_code;
pub use ir::{CodeIr, IrOp, IrStep};
pub use spec::{CodePattern, EmitNode, Formatting, LanguageSpec, Placeholder};
