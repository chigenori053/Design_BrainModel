use std::fmt;

use crate::ir::IrOp;
use crate::spec::Placeholder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    MissingPattern(IrOp),
    UnresolvedPlaceholder(Placeholder),
    InvalidEmitNode(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::MissingPattern(op) => {
                write!(f, "missing pattern for op: {}", op)
            }
            CodegenError::UnresolvedPlaceholder(p) => {
                write!(f, "unresolved placeholder: {}", p)
            }
            CodegenError::InvalidEmitNode(msg) => {
                write!(f, "invalid emit node: {}", msg)
            }
        }
    }
}

impl std::error::Error for CodegenError {}
