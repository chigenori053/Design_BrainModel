use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodegenError {
    EmptyFunctionName,
    EmptyParamName { function: String },
    DuplicateParam { function: String, param: String },
    DuplicateBinding { name: String, depth: usize },
    UnresolvedVariable { name: String },
    UnsupportedTypeRendering { ty: String, language: String },
    MissingFunctionPattern { language: String },
    InvalidReturnType,
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFunctionName =>
                write!(f, "function name must not be empty"),
            Self::EmptyParamName { function } =>
                write!(f, "param name must not be empty in function '{function}'"),
            Self::DuplicateParam { function, param } =>
                write!(f, "duplicate param '{param}' in function '{function}'"),
            Self::DuplicateBinding { name, depth } =>
                write!(f, "duplicate binding '{name}' at scope depth {depth}"),
            Self::UnresolvedVariable { name } =>
                write!(f, "unresolved variable '{name}'"),
            Self::UnsupportedTypeRendering { ty, language } =>
                write!(f, "cannot render type '{ty}' for language '{language}'"),
            Self::MissingFunctionPattern { language } =>
                write!(f, "no function pattern defined for language '{language}'"),
            Self::InvalidReturnType =>
                write!(f, "invalid return type"),
        }
    }
}

impl std::error::Error for CodegenError {}
