use std::fmt;

use crate::ir::IrOp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placeholder {
    VarName,
    ValueExpr,
    FuncName,
    Args,
    ConditionExpr,
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Placeholder::VarName => write!(f, "VarName"),
            Placeholder::ValueExpr => write!(f, "ValueExpr"),
            Placeholder::FuncName => write!(f, "FuncName"),
            Placeholder::Args => write!(f, "Args"),
            Placeholder::ConditionExpr => write!(f, "ConditionExpr"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EmitNode {
    Text(String),
    Placeholder(Placeholder),
    Sequence(Vec<EmitNode>),
    Join { items: Vec<EmitNode>, separator: String },
    Optional { condition: String, node: Box<EmitNode> },
    Indent(Box<EmitNode>),
    NewLine,
}

#[derive(Debug, Clone)]
pub struct CodePattern {
    pub emit: EmitNode,
}

#[derive(Debug, Clone)]
pub struct Formatting {
    pub newline: String,
    pub indent: String,
}

impl Default for Formatting {
    fn default() -> Self {
        Self {
            newline: "\n".to_string(),
            indent: "    ".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LanguageSpec {
    /// Ordered Vec instead of HashMap to guarantee deterministic traversal.
    pub patterns: Vec<(IrOp, CodePattern)>,
    pub formatting: Formatting,
}

impl LanguageSpec {
    pub fn new(patterns: Vec<(IrOp, CodePattern)>, formatting: Formatting) -> Self {
        Self { patterns, formatting }
    }

    pub fn find_pattern(&self, op: &IrOp) -> Option<&CodePattern> {
        self.patterns.iter().find(|(k, _)| k == op).map(|(_, v)| v)
    }
}
