#![allow(dead_code)]

use codegen_core::*;

pub fn assign_step(var: &str, val: &str) -> IrStep {
    IrStep {
        op: IrOp::Assign,
        target: None,
        inputs: vec![val.to_string()],
        outputs: vec![var.to_string()],
        condition: None,
    }
}

pub fn call_step(func: &str, args: &[&str]) -> IrStep {
    IrStep {
        op: IrOp::Call,
        target: Some(func.to_string()),
        inputs: args.iter().map(|s| s.to_string()).collect(),
        outputs: vec![],
        condition: None,
    }
}

pub fn rust_spec() -> LanguageSpec {
    LanguageSpec::new(
        vec![
            (
                IrOp::Assign,
                CodePattern {
                    emit: EmitNode::Sequence(vec![
                        EmitNode::Text("let ".to_string()),
                        EmitNode::Placeholder(Placeholder::VarName),
                        EmitNode::Text(" = ".to_string()),
                        EmitNode::Placeholder(Placeholder::ValueExpr),
                        EmitNode::Text(";".to_string()),
                        EmitNode::NewLine,
                    ]),
                },
            ),
            (
                IrOp::Call,
                CodePattern {
                    emit: EmitNode::Sequence(vec![
                        EmitNode::Placeholder(Placeholder::FuncName),
                        EmitNode::Text("(".to_string()),
                        EmitNode::Placeholder(Placeholder::Args),
                        EmitNode::Text(");".to_string()),
                        EmitNode::NewLine,
                    ]),
                },
            ),
        ],
        Formatting::default(),
    )
}

pub fn python_spec() -> LanguageSpec {
    LanguageSpec::new(
        vec![
            (
                IrOp::Assign,
                CodePattern {
                    emit: EmitNode::Sequence(vec![
                        EmitNode::Placeholder(Placeholder::VarName),
                        EmitNode::Text(" = ".to_string()),
                        EmitNode::Placeholder(Placeholder::ValueExpr),
                        EmitNode::NewLine,
                    ]),
                },
            ),
            (
                IrOp::Call,
                CodePattern {
                    emit: EmitNode::Sequence(vec![
                        EmitNode::Placeholder(Placeholder::FuncName),
                        EmitNode::Text("(".to_string()),
                        EmitNode::Placeholder(Placeholder::Args),
                        EmitNode::Text(")".to_string()),
                        EmitNode::NewLine,
                    ]),
                },
            ),
        ],
        Formatting::default(),
    )
}
