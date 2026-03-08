use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemorySpaceError {
    EmptyField,
    DimensionMismatch { left: usize, right: usize },
}

impl fmt::Display for MemorySpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField => write!(f, "field must not be empty"),
            Self::DimensionMismatch { left, right } => {
                write!(f, "dimension mismatch: left={left}, right={right}")
            }
        }
    }
}

impl std::error::Error for MemorySpaceError {}
