#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstModule {
    pub name: String,
    pub imports: Vec<String>,
    pub functions: Vec<AstFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstFunction {
    pub name: String,
    pub params: Vec<(String, Option<String>)>,
    pub return_type: Option<String>,
    pub body: AstBlock,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AstBlock {
    pub statements: Vec<AstStatement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstNode {
    Function(AstFunction),
    Assign,
    Call,
    Return,
    If,
    Loop,
    Block(AstBlock),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstStatement {
    Assign {
        target: String,
        value: AstExpression,
    },
    Call(AstExpression),
    Return(Option<AstExpression>),
    If {
        condition: AstExpression,
        then_block: AstBlock,
        else_block: AstBlock,
    },
    Loop(AstLoop),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstLoop {
    pub kind: AstLoopKind,
    pub iterator: AstExpression,
    pub body: AstBlock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstLoopKind {
    For,
    While,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstExpression {
    Literal(AstLiteral),
    Variable(String),
    Call {
        function: String,
        args: Vec<AstExpression>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstLiteral {
    Int(i64),
    Bool(bool),
    String(String),
    Void,
}
