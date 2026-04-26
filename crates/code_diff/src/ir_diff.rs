use code_ir::program_v1::{Function, Module, Program, Statement};

use crate::{change_set::ChangeSet, matcher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IrChange {
    AddFunction {
        module: String,
        function: Function,
    },
    RemoveFunction {
        module: String,
        function_name: String,
    },
    ModifyFunction {
        module: String,
        old_name: String,
        new_function: Function,
    },
    AddStatement {
        module: String,
        function_name: String,
        index: usize,
        statement: Statement,
    },
    RemoveStatement {
        module: String,
        function_name: String,
        index: usize,
    },
    ModifyStatement {
        module: String,
        function_name: String,
        index: usize,
        new_statement: Statement,
    },
    RenameSymbol {
        module: String,
        function_name: String,
        old_name: String,
        new_name: String,
    },
}

pub fn diff_programs(old: &Program, new: &Program) -> ChangeSet {
    let mut changes = Vec::new();
    let mut old_modules = old.modules.iter().collect::<Vec<_>>();
    let mut new_modules = new.modules.iter().collect::<Vec<_>>();
    old_modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    new_modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    for old_module in &old_modules {
        let Some(new_module) = new_modules
            .iter()
            .find(|module| module.name == old_module.name)
        else {
            for function in &old_module.functions {
                changes.push(IrChange::RemoveFunction {
                    module: old_module.name.clone(),
                    function_name: function.name.clone(),
                });
            }
            continue;
        };
        diff_module(old_module, new_module, &mut changes);
    }

    for new_module in &new_modules {
        if old_modules
            .iter()
            .any(|module| module.name == new_module.name)
        {
            continue;
        }
        for function in &new_module.functions {
            changes.push(IrChange::AddFunction {
                module: new_module.name.clone(),
                function: function.clone(),
            });
        }
    }

    ChangeSet::new(changes)
}

fn diff_module(old: &Module, new: &Module, changes: &mut Vec<IrChange>) {
    let matches = matcher::match_functions(old, new);
    let matched_old_names = matches
        .iter()
        .map(|matched| matched.old.name.as_str())
        .collect::<Vec<_>>();
    let matched_new_names = matches
        .iter()
        .map(|matched| matched.new.name.as_str())
        .collect::<Vec<_>>();

    for function in &old.functions {
        if matched_old_names.contains(&function.name.as_str()) {
            continue;
        }
        changes.push(IrChange::RemoveFunction {
            module: old.name.clone(),
            function_name: function.name.clone(),
        });
    }

    for function in &new.functions {
        if matched_new_names.contains(&function.name.as_str()) {
            continue;
        }
        changes.push(IrChange::AddFunction {
            module: new.name.clone(),
            function: function.clone(),
        });
    }

    for matched in matches {
        diff_function(old.name.as_str(), matched.old, matched.new, changes);
    }
}

fn diff_function(module: &str, old: &Function, new: &Function, changes: &mut Vec<IrChange>) {
    if old.name != new.name {
        changes.push(IrChange::RenameSymbol {
            module: module.to_string(),
            function_name: old.name.clone(),
            old_name: old.name.clone(),
            new_name: new.name.clone(),
        });
    }

    if old.inputs != new.inputs || old.outputs != new.outputs {
        changes.push(IrChange::ModifyFunction {
            module: module.to_string(),
            old_name: old.name.clone(),
            new_function: new.clone(),
        });
        return;
    }

    if let Some((old_name, new_name)) = detect_function_rename(old, new) {
        changes.push(IrChange::RenameSymbol {
            module: module.to_string(),
            function_name: new.name.clone(),
            old_name,
            new_name,
        });
        return;
    }

    let max_len = old.body.statements.len().max(new.body.statements.len());
    for index in 0..max_len {
        match (
            old.body.statements.get(index),
            new.body.statements.get(index),
        ) {
            (Some(old_stmt), Some(new_stmt)) if old_stmt == new_stmt => {}
            (Some(old_stmt), Some(new_stmt)) => {
                if let Some((old_name, new_name)) = detect_rename(old_stmt, new_stmt) {
                    changes.push(IrChange::RenameSymbol {
                        module: module.to_string(),
                        function_name: new.name.clone(),
                        old_name,
                        new_name,
                    });
                } else {
                    changes.push(IrChange::ModifyStatement {
                        module: module.to_string(),
                        function_name: new.name.clone(),
                        index,
                        new_statement: new_stmt.clone(),
                    });
                }
            }
            (None, Some(new_stmt)) => changes.push(IrChange::AddStatement {
                module: module.to_string(),
                function_name: new.name.clone(),
                index,
                statement: new_stmt.clone(),
            }),
            (Some(_), None) => changes.push(IrChange::RemoveStatement {
                module: module.to_string(),
                function_name: new.name.clone(),
                index,
            }),
            (None, None) => {}
        }
    }
}

fn detect_function_rename(old: &Function, new: &Function) -> Option<(String, String)> {
    if old.body.statements.len() != new.body.statements.len() {
        return None;
    }
    let mut candidate: Option<(String, String)> = None;
    for (old_stmt, new_stmt) in old.body.statements.iter().zip(&new.body.statements) {
        let rename = statement_rename_candidate(old_stmt, new_stmt)?;
        match &candidate {
            Some(existing) if existing.0.is_empty() && existing.1.is_empty() => {
                candidate = Some(rename)
            }
            Some(_) if rename.0.is_empty() && rename.1.is_empty() => {}
            Some(existing) if existing != &rename => return None,
            None => candidate = Some(rename),
            Some(_) => {}
        }
    }
    let candidate = candidate?;
    if candidate.0.is_empty() && candidate.1.is_empty() {
        return None;
    }
    let mut renamed = old.body.statements.clone();
    rename_in_statements(&mut renamed, &candidate.0, &candidate.1);
    if renamed == new.body.statements {
        Some(candidate)
    } else {
        None
    }
}

fn detect_rename(old: &Statement, new: &Statement) -> Option<(String, String)> {
    match (old, new) {
        (
            Statement::Assign {
                target: old_target,
                value: old_value,
            },
            Statement::Assign {
                target: new_target,
                value: new_value,
            },
        ) if old_value == new_value && old_target != new_target => {
            Some((old_target.clone(), new_target.clone()))
        }
        _ => None,
    }
}

fn statement_rename_candidate(old: &Statement, new: &Statement) -> Option<(String, String)> {
    match (old, new) {
        (
            Statement::Assign {
                target: old_target,
                value: old_value,
            },
            Statement::Assign {
                target: new_target,
                value: new_value,
            },
        ) => expression_rename_candidate(old_value, new_value).or_else(|| {
            if old_target != new_target {
                Some((old_target.clone(), new_target.clone()))
            } else {
                None
            }
        }),
        (
            Statement::Return {
                value: Some(old_value),
            },
            Statement::Return {
                value: Some(new_value),
            },
        ) => expression_rename_candidate(old_value, new_value),
        (Statement::Expression(old_expr), Statement::Expression(new_expr)) => {
            expression_rename_candidate(old_expr, new_expr)
        }
        _ if old == new => Some((String::new(), String::new())),
        _ => None,
    }
}

fn expression_rename_candidate(
    old: &code_ir::program_v1::Expression,
    new: &code_ir::program_v1::Expression,
) -> Option<(String, String)> {
    match (old, new) {
        (
            code_ir::program_v1::Expression::Variable(old_name),
            code_ir::program_v1::Expression::Variable(new_name),
        ) if old_name != new_name => Some((old_name.clone(), new_name.clone())),
        (
            code_ir::program_v1::Expression::Call(old_call),
            code_ir::program_v1::Expression::Call(new_call),
        ) if old_call.function == new_call.function
            && old_call.args.len() == new_call.args.len() =>
        {
            let mut candidate = None;
            for (old_arg, new_arg) in old_call.args.iter().zip(&new_call.args) {
                let rename = expression_rename_candidate(old_arg, new_arg)?;
                match &candidate {
                    Some(existing) if existing != &rename => return None,
                    None => candidate = Some(rename),
                    Some(_) => {}
                }
            }
            candidate
        }
        _ if old == new => Some((String::new(), String::new())),
        _ => None,
    }
}

pub fn replay_changes(old: &Program, changes: &ChangeSet) -> Result<Program, ReplayError> {
    let mut next = old.clone();
    for change in &changes.changes {
        apply_change(&mut next, change)?;
    }
    Ok(next)
}

fn apply_change(program: &mut Program, change: &IrChange) -> Result<(), ReplayError> {
    match change {
        IrChange::AddFunction { module, function } => {
            let module = find_or_create_module(program, module);
            if module
                .functions
                .iter()
                .any(|candidate| candidate.name == function.name)
            {
                return Err(ReplayError::Conflict(format!(
                    "function {} already exists",
                    function.name
                )));
            }
            module.functions.push(function.clone());
            module.functions.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
        }
        IrChange::RemoveFunction {
            module,
            function_name,
        } => {
            let module = find_module_mut(program, module)?;
            let before = module.functions.len();
            module
                .functions
                .retain(|function| function.name != *function_name);
            if before == module.functions.len() {
                return Err(ReplayError::Missing(format!(
                    "function {} not found",
                    function_name
                )));
            }
        }
        IrChange::ModifyFunction {
            module,
            old_name,
            new_function,
        } => {
            let function = find_function_mut(program, module, old_name)?;
            *function = new_function.clone();
        }
        IrChange::AddStatement {
            module,
            function_name,
            index,
            statement,
        } => {
            let function = find_function_mut(program, module, function_name)?;
            if *index > function.body.statements.len() {
                return Err(ReplayError::InvalidIndex(*index));
            }
            function.body.statements.insert(*index, statement.clone());
        }
        IrChange::RemoveStatement {
            module,
            function_name,
            index,
        } => {
            let function = find_function_mut(program, module, function_name)?;
            if *index >= function.body.statements.len() {
                return Err(ReplayError::InvalidIndex(*index));
            }
            function.body.statements.remove(*index);
        }
        IrChange::ModifyStatement {
            module,
            function_name,
            index,
            new_statement,
        } => {
            let function = find_function_mut(program, module, function_name)?;
            if *index >= function.body.statements.len() {
                return Err(ReplayError::InvalidIndex(*index));
            }
            function.body.statements[*index] = new_statement.clone();
        }
        IrChange::RenameSymbol {
            module,
            function_name,
            old_name,
            new_name,
        } => {
            let function = find_function_mut(program, module, function_name)?;
            if function.name == *old_name {
                function.name = new_name.clone();
            }
            rename_in_statements(&mut function.body.statements, old_name, new_name);
            for input in &mut function.inputs {
                if input.name == *old_name {
                    input.name = new_name.clone();
                }
            }
        }
    }
    Ok(())
}

fn rename_in_statements(statements: &mut [Statement], old_name: &str, new_name: &str) {
    for statement in statements {
        rename_in_statement(statement, old_name, new_name);
    }
}

fn rename_in_statement(statement: &mut Statement, old_name: &str, new_name: &str) {
    match statement {
        Statement::Assign { target, value } => {
            if target == old_name {
                *target = new_name.to_string();
            }
            rename_in_expression(value, old_name, new_name);
        }
        Statement::If(if_stmt) => {
            rename_in_expression(&mut if_stmt.condition, old_name, new_name);
            rename_in_statements(&mut if_stmt.then_block.statements, old_name, new_name);
            rename_in_statements(&mut if_stmt.else_block.statements, old_name, new_name);
        }
        Statement::Loop(loop_stmt) => {
            rename_in_expression(&mut loop_stmt.iterator, old_name, new_name);
            rename_in_statements(&mut loop_stmt.body.statements, old_name, new_name);
        }
        Statement::Return { value } => {
            if let Some(value) = value {
                rename_in_expression(value, old_name, new_name);
            }
        }
        Statement::Expression(expr) => rename_in_expression(expr, old_name, new_name),
    }
}

fn rename_in_expression(
    expression: &mut code_ir::program_v1::Expression,
    old_name: &str,
    new_name: &str,
) {
    match expression {
        code_ir::program_v1::Expression::Variable(name) => {
            if name == old_name {
                *name = new_name.to_string();
            }
        }
        code_ir::program_v1::Expression::Call(call) => {
            if call.function == old_name {
                call.function = new_name.to_string();
            }
            for arg in &mut call.args {
                rename_in_expression(arg, old_name, new_name);
            }
        }
        code_ir::program_v1::Expression::Await(expr) => {
            rename_in_expression(expr, old_name, new_name)
        }
        code_ir::program_v1::Expression::BinaryOp(op) => {
            rename_in_expression(&mut op.left, old_name, new_name);
            rename_in_expression(&mut op.right, old_name, new_name);
        }
        code_ir::program_v1::Expression::UnaryOp(op) => {
            rename_in_expression(&mut op.expr, old_name, new_name);
        }
        code_ir::program_v1::Expression::Literal(_) => {}
    }
}

fn find_or_create_module<'a>(program: &'a mut Program, module_name: &str) -> &'a mut Module {
    if let Some(index) = program
        .modules
        .iter()
        .position(|module| module.name == module_name)
    {
        return &mut program.modules[index];
    }
    program.modules.push(Module {
        name: module_name.to_string(),
        visibility: code_ir::program_v1::Visibility::Public,
        imports: vec![],
        types: vec![],
        functions: vec![],
        state: vec![],
    });
    program.modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    let index = program
        .modules
        .iter()
        .position(|module| module.name == module_name)
        .expect("module inserted");
    &mut program.modules[index]
}

fn find_module_mut<'a>(
    program: &'a mut Program,
    module_name: &str,
) -> Result<&'a mut Module, ReplayError> {
    program
        .modules
        .iter_mut()
        .find(|module| module.name == module_name)
        .ok_or_else(|| ReplayError::Missing(format!("module {} not found", module_name)))
}

fn find_function_mut<'a>(
    program: &'a mut Program,
    module_name: &str,
    function_name: &str,
) -> Result<&'a mut Function, ReplayError> {
    let module = find_module_mut(program, module_name)?;
    module
        .functions
        .iter_mut()
        .find(|function| function.name == function_name)
        .ok_or_else(|| ReplayError::Missing(format!("function {} not found", function_name)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Missing(String),
    InvalidIndex(usize),
    Conflict(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(message) | Self::Conflict(message) => write!(f, "{message}"),
            Self::InvalidIndex(index) => write!(f, "invalid index: {index}"),
        }
    }
}

impl std::error::Error for ReplayError {}
