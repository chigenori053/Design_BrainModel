/// Placeholder slots filled by the emitter when walking IrStep / IrFunction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placeholder {
    // Step-level (Step3)
    VarName,
    ValueExpr,
    FuncName,
    Args,
    ConditionExpr,
    Body,
    ElseBody,
    // Function-level (Step4)
    ParamList,
    ReturnType,
    TypeName,
    FunctionBody,
}

/// Structural nodes used to describe how a language construct is emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitNode {
    Text(String),
    Placeholder(Placeholder),
    Sequence(Vec<EmitNode>),
    Join { nodes: Vec<EmitNode>, sep: String },
    Optional(Box<EmitNode>),
    Indent(Box<EmitNode>),
    NewLine,
    Block { body: Vec<EmitNode> },
    Branch {
        condition: Placeholder,
        then_body: Vec<EmitNode>,
        else_body: Option<Vec<EmitNode>>,
    },
    Loop { body: Vec<EmitNode> },
}
