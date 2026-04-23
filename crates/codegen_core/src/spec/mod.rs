use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod loader;
pub mod validator;

#[cfg(test)]
mod tests;

/// All IR operations that a LanguageSpec must provide patterns for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IrOp {
    Assign,
    Call,
    Return,
    IfElse,
    Loop,
    FuncDef,
    StructDef,
}

impl IrOp {
    pub fn all() -> &'static [IrOp] {
        &[
            IrOp::Assign,
            IrOp::Call,
            IrOp::Return,
            IrOp::IfElse,
            IrOp::Loop,
            IrOp::FuncDef,
            IrOp::StructDef,
        ]
    }
}

/// Language-specific code generation specification.
/// Immutable once loaded; same IR + same Spec → same output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageSpec {
    pub language: String,
    pub version: String,
    pub patterns: HashMap<IrOp, Pattern>,
    pub formatting: FormattingRules,
    pub features: FeatureFlags,
}

/// Structural code pattern for a single IrOp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub emit: Vec<EmitNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternKind {
    Statement,
    Expression,
    Block,
}

/// Structural emission node — never a raw string template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EmitNode {
    /// Literal text emitted verbatim.
    Text(String),

    /// Slot resolved from the current IR node at render time.
    Placeholder(Placeholder),

    /// Ordered concatenation of child nodes.
    Sequence(Vec<EmitNode>),

    /// Joins a list placeholder with a separator string.
    Join {
        items: Placeholder,
        separator: String,
    },

    /// Emits `node` only when `condition` is satisfied.
    Optional {
        condition: EmitCondition,
        node: Box<EmitNode>,
    },

    /// Increases indent_level by 1 for the wrapped node.
    Indent(Box<EmitNode>),

    /// Outputs `formatting.newline`.
    NewLine,
}

/// IR value slots that can be resolved during emission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Placeholder {
    VarName,
    ValueExpr,
    Type,
    FuncName,
    Args,
    ConditionExpr,
}

/// Predicate evaluated against EmitContext at render time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EmitCondition {
    HasType,
    InBlock,
    /// Name must match a field on FeatureFlags.
    FeatureEnabled(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormattingRules {
    pub indent: String,
    pub newline: String,
    pub semicolon: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub requires_semicolon: bool,
    pub has_type_annotations: bool,
    pub supports_block_scope: bool,
}

/// Runtime context threaded through an emission pass.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitContext {
    pub indent_level: usize,
    pub in_block: bool,
    pub features: FeatureFlags,
}
