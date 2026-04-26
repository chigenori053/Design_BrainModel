use code_ir::program_v1::{
    Block, Function, FunctionInput, FunctionOutput, GenerationMode, GenerationStrategy, Module,
    Program, SafetyPolicy, Statement, TypeRef, Visibility,
};

use crate::{
    ParseError,
    ast::{AstBlock, AstExpression, AstLiteral, AstLoopKind, AstModule, AstStatement},
};

pub fn build_ir(ast: AstModule) -> Result<Program, ParseError> {
    let module_name = normalize_module_name(&ast.name);
    let functions = ast
        .functions
        .into_iter()
        .map(|function| {
            Ok(Function {
                name: function.name,
                visibility: Visibility::Public,
                inputs: function
                    .params
                    .into_iter()
                    .map(|(name, ty)| FunctionInput {
                        name,
                        r#type: ty
                            .map(TypeRef::named)
                            .unwrap_or_else(|| TypeRef::named("Unknown")),
                        borrow: None,
                    })
                    .collect(),
                outputs: FunctionOutput {
                    r#type: function
                        .return_type
                        .map(TypeRef::named)
                        .unwrap_or_else(TypeRef::void),
                },
                effects: vec![],
                can_fail: false,
                body: build_block(function.body)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Program {
        metadata: code_ir::program_v1::Metadata {
            name: module_name.clone(),
            version: "1.0.0".to_string(),
            target_domains: vec![code_ir::program_v1::TargetDomain::Backend],
        },
        modules: vec![Module {
            name: module_name,
            visibility: Visibility::Public,
            imports: normalize_imports(ast.imports),
            types: vec![],
            functions,
            state: vec![],
        }],
        dependencies: vec![],
        generation_strategy: GenerationStrategy {
            mode: GenerationMode::DryRun,
            safety: SafetyPolicy {
                backup: true,
                check: true,
                rollback_on_fail: true,
            },
        },
        build_validation: code_ir::program_v1::BuildValidation {
            enabled: false,
            command: "none".to_string(),
            sandbox: true,
        },
    })
}

fn build_block(block: AstBlock) -> Result<Block, ParseError> {
    let statements = block
        .statements
        .into_iter()
        .map(build_statement)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Block { statements })
}

fn build_statement(statement: AstStatement) -> Result<Statement, ParseError> {
    Ok(match statement {
        AstStatement::Assign { target, value } => Statement::Assign {
            target,
            value: build_expression(value)?,
        },
        AstStatement::Call(expr) => Statement::Expression(build_expression(expr)?),
        AstStatement::Return(value) => Statement::Return {
            value: value.map(build_expression).transpose()?,
        },
        AstStatement::If {
            condition,
            then_block,
            else_block,
        } => Statement::If(code_ir::program_v1::IfStatement {
            condition: build_expression(condition)?,
            then_block: build_block(then_block)?,
            else_block: build_block(else_block)?,
        }),
        AstStatement::Loop(loop_stmt) => Statement::Loop(code_ir::program_v1::LoopStatement {
            kind: match loop_stmt.kind {
                AstLoopKind::For => code_ir::program_v1::LoopKind::For,
                AstLoopKind::While => code_ir::program_v1::LoopKind::While,
            },
            iterator: build_expression(loop_stmt.iterator)?,
            body: build_block(loop_stmt.body)?,
        }),
    })
}

fn build_expression(
    expression: AstExpression,
) -> Result<code_ir::program_v1::Expression, ParseError> {
    Ok(match expression {
        AstExpression::Literal(literal) => {
            code_ir::program_v1::Expression::Literal(match literal {
                AstLiteral::Int(value) => code_ir::program_v1::Literal::Int(value),
                AstLiteral::Bool(value) => code_ir::program_v1::Literal::Bool(value),
                AstLiteral::String(value) => code_ir::program_v1::Literal::String(value),
                AstLiteral::Void => code_ir::program_v1::Literal::Void,
            })
        }
        AstExpression::Variable(name) => code_ir::program_v1::Expression::Variable(name),
        AstExpression::Call { function, args } => {
            code_ir::program_v1::Expression::Call(code_ir::program_v1::Call {
                function,
                args: args
                    .into_iter()
                    .map(build_expression)
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
    })
}

fn normalize_imports(mut imports: Vec<String>) -> Vec<String> {
    imports.sort();
    imports.dedup();
    imports
}

fn normalize_module_name(name: &str) -> String {
    let mut normalized = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if !normalized.ends_with('_') {
            normalized.push('_');
        }
    }
    let normalized = normalized.trim_matches('_');
    if normalized.is_empty() {
        "module".to_string()
    } else {
        normalized.to_string()
    }
}
