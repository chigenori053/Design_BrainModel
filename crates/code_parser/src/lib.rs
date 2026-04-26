pub mod ast;
pub mod ir_builder;
pub mod python;
pub mod rust;

use code_ir::program_v1::{BackendLanguage, Program};

pub use ast::{
    AstBlock, AstExpression, AstFunction, AstLiteral, AstLoop, AstLoopKind, AstModule, AstNode,
    AstStatement,
};
pub use ir_builder::build_ir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportedLanguage {
    Rust,
    Python,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    ParseError,
    UnsupportedSyntax,
    AmbiguousConstruct,
    IRBuildError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
    pub line: Option<usize>,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, message: impl Into<String>, line: Option<usize>) -> Self {
        Self {
            kind,
            message: message.into(),
            line,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{:?} at line {}: {}", self.kind, line, self.message),
            None => write!(f, "{:?}: {}", self.kind, self.message),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_source_to_ast(
    language: SupportedLanguage,
    module_name: &str,
    source: &str,
) -> Result<AstModule, ParseError> {
    match language {
        SupportedLanguage::Rust => rust::parse_module(module_name, source),
        SupportedLanguage::Python => python::parse_module(module_name, source),
    }
}

pub fn parse_source_to_ir(
    language: SupportedLanguage,
    module_name: &str,
    source: &str,
) -> Result<Program, ParseError> {
    let ast = parse_source_to_ast(language, module_name, source)?;
    build_ir(ast)
}

pub fn roundtrip_ir(language: SupportedLanguage, program: &Program) -> Result<Program, ParseError> {
    let files = program.render_canonical_source_tree(match language {
        SupportedLanguage::Rust => BackendLanguage::Rust,
        SupportedLanguage::Python => BackendLanguage::Python,
    });
    if files.len() != 1 {
        return Err(ParseError::new(
            ParseErrorKind::AmbiguousConstruct,
            "roundtrip expects exactly one module file",
            None,
        ));
    }
    let (path, source) = &files[0];
    let module_name = path
        .rsplit('/')
        .next()
        .unwrap_or(path.as_str())
        .split('.')
        .next()
        .unwrap_or(path.as_str());
    parse_source_to_ir(language, module_name, source)
}
