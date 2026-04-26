use code_diff::{ChangeSet, IrChange};
use code_ir::program_v1::Program;

pub fn rename_variable(
    program: &Program,
    module_name: &str,
    function_name: &str,
    old_name: &str,
    new_name: &str,
) -> Result<ChangeSet, RenameVariableError> {
    let module = program
        .modules
        .iter()
        .find(|module| module.name == module_name)
        .ok_or_else(|| RenameVariableError::MissingModule(module_name.to_string()))?;
    let function = module
        .functions
        .iter()
        .find(|function| function.name == function_name)
        .ok_or_else(|| RenameVariableError::MissingFunction(function_name.to_string()))?;

    let exists_in_inputs = function.inputs.iter().any(|input| input.name == old_name);
    let exists_in_assignments = function.body.statements.iter().any(|statement| {
        matches!(statement, code_ir::program_v1::Statement::Assign { target, .. } if target == old_name)
    });
    if !exists_in_inputs && !exists_in_assignments {
        return Err(RenameVariableError::MissingSymbol(old_name.to_string()));
    }
    if old_name == new_name {
        return Err(RenameVariableError::NoOp);
    }

    Ok(ChangeSet::new(vec![IrChange::RenameSymbol {
        module: module_name.to_string(),
        function_name: function_name.to_string(),
        old_name: old_name.to_string(),
        new_name: new_name.to_string(),
    }]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameVariableError {
    MissingModule(String),
    MissingFunction(String),
    MissingSymbol(String),
    NoOp,
}

impl std::fmt::Display for RenameVariableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModule(module) => write!(f, "module not found: {module}"),
            Self::MissingFunction(function) => write!(f, "function not found: {function}"),
            Self::MissingSymbol(symbol) => write!(f, "symbol not found: {symbol}"),
            Self::NoOp => write!(f, "rename is a no-op"),
        }
    }
}

impl std::error::Error for RenameVariableError {}
