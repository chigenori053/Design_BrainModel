use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemorySpaceError {
    EmptyField,
    DimensionMismatch { left: usize, right: usize },
    MissingCanonicalMemory(u64),
    TransitionHashMismatch { expected: u64, actual: u64 },
    UnsafeTransitionMerge,
    UnsafeSemanticMerge(String),
}

impl fmt::Display for MemorySpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField => write!(f, "field must not be empty"),
            Self::DimensionMismatch { left, right } => {
                write!(f, "dimension mismatch: left={left}, right={right}")
            }
            Self::MissingCanonicalMemory(memory_id) => {
                write!(f, "missing canonical memory: memory_id={memory_id}")
            }
            Self::TransitionHashMismatch { expected, actual } => {
                write!(
                    f,
                    "transition hash mismatch: expected={expected} actual={actual}"
                )
            }
            Self::UnsafeTransitionMerge => write!(
                f,
                "unsafe transition merge: semantic equality is not state equality"
            ),
            Self::UnsafeSemanticMerge(reason) => {
                write!(f, "unsafe semantic merge: {reason}")
            }
        }
    }
}

impl std::error::Error for MemorySpaceError {}
